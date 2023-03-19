use std::iter;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::AsyncWriteExt;
use futures::{AsyncRead, AsyncWrite, StreamExt};
use futures_util::AsyncReadExt;
use libp2p::core::{muxing::StreamMuxerBox, transport::Boxed};
use libp2p::gossipsub::{IdentTopic as Topic, MessageAuthenticity, ValidationMode};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::tcp::{tokio::Transport as TokioTransport, Config as TokioConfig};
use libp2p::yamux::YamuxConfig;
use libp2p::{
    gossipsub, noise, request_response, Multiaddr, PeerId as Libp2pPeerId, Swarm, Transport,
};
use serde::{Deserialize, Serialize};
use tokio::select;

use crate::block::types::message::EphemeraMessage;
use crate::broadcast::RbMsg;
use crate::config::{Libp2pConfig, NodeConfig};
use crate::core::builder::NodeInfo;
use crate::network::libp2p::discovery::r#static::StaticPeerDiscovery;
use crate::network::libp2p::messages_channel::{
    EphemeraNetworkCommunication, NetCommunicationReceiver, NetCommunicationSender,
};
use crate::utilities::crypto::ed25519::Ed25519Keypair;

pub struct SwarmNetwork {
    libp2p_conf: Libp2pConfig,
    node_conf: NodeConfig,
    swarm: Swarm<GroupNetworkBehaviour>,
    from_ephemera_rcv: NetCommunicationReceiver,
    to_ephemera_tx: NetCommunicationSender,
}

impl SwarmNetwork {
    pub(crate) fn new(
        libp2p_conf: Libp2pConfig,
        node_conf: NodeConfig,
        node_info: NodeInfo,
    ) -> (
        SwarmNetwork,
        NetCommunicationReceiver,
        NetCommunicationSender,
    ) {
        let (from_ephemera_tx, from_ephemera_rcv) = EphemeraNetworkCommunication::init();
        let (to_ephemera_tx, to_ephemera_rcv) = EphemeraNetworkCommunication::init();

        let local_key = node_info.keypair.clone();
        let peer_id = node_info.peer_id.0;

        let transport = create_transport(local_key.clone());
        let behaviour = create_behaviour(&libp2p_conf, local_key);
        let swarm = Swarm::with_tokio_executor(transport, behaviour, peer_id);

        let network = SwarmNetwork {
            libp2p_conf,
            node_conf,
            swarm,
            from_ephemera_rcv,
            to_ephemera_tx,
        };

        (network, to_ephemera_rcv, from_ephemera_tx)
    }

    pub(crate) fn listen(&mut self) -> anyhow::Result<()> {
        let address =
            Multiaddr::from_str(self.node_conf.address.as_str()).expect("Invalid multi-address");
        self.swarm.listen_on(address.clone())?;

        log::info!("Listening on {address:?}");
        Ok(())
    }

    pub(crate) async fn start(mut self) -> anyhow::Result<()> {
        let consensus_msg_topic = Topic::new(&self.libp2p_conf.consensus_msg_topic_name);
        let ephemera_msg_topic = Topic::new(&self.libp2p_conf.proposed_msg_topic_name);

        loop {
            select!(
                swarm_event = self.swarm.next() => {
                    match swarm_event{
                        Some(event) => {
                            if let Err(err) = self.handle_incoming_messages(event, &consensus_msg_topic, &ephemera_msg_topic).await{
                                log::error!("Error handling swarm event: {:?}", err);
                            }
                        }
                        None => {
                            anyhow::bail!("Swarm event channel closed")
                        }
                    }
                },
                cm_maybe = self.from_ephemera_rcv.ephemera_message_receiver.recv() => {
                    if let Some(cm) = cm_maybe {
                        self.send_ephemera_message(cm, &ephemera_msg_topic,).await;
                    }
                    else {
                        anyhow::bail!("ephemera_message_receiver channel closed")
                    }
                }
                pm_maybe = self.from_ephemera_rcv.protocol_msg_receiver.recv() => {
                    if let Some(pm) = pm_maybe {
                        self.send_protocol_message(pm).await;
                    }
                    else {
                        anyhow::bail!("protocol_msg_receiver channel closed")
                    }
                }
            );
        }
    }

    #[allow(clippy::collapsible_match, clippy::single_match)]
    async fn handle_incoming_messages<E>(
        &mut self,
        swarm_event: SwarmEvent<GroupBehaviourEvent, E>,
        protocol_msg_topic: &Topic,
        ephemera_msg_topic: &Topic,
    ) -> anyhow::Result<()> {
        match swarm_event {
            SwarmEvent::Behaviour(b) => match b {
                GroupBehaviourEvent::Gossipsub(gs) => match gs {
                    gossipsub::Event::Message {
                        propagation_source: _,
                        message_id: _,
                        message,
                    } => {
                        if message.topic == (*protocol_msg_topic).clone().into() {
                            self.to_ephemera_tx
                                .send_protocol_message_raw(message.data)
                                .await?;
                        } else if message.topic == (*ephemera_msg_topic).clone().into() {
                            self.to_ephemera_tx
                                .send_ephemera_message_raw(message.data)
                                .await?;
                        }
                    }
                    _ => {}
                },
                GroupBehaviourEvent::RequestResponse(request_response) => {
                    match request_response {
                        request_response::Event::Message { peer, message } => match message {
                            request_response::Message::Request {
                                request_id: _,
                                request,
                                channel,
                            } => {
                                let rb_id = request.id.clone();
                                self.to_ephemera_tx.send_protocol_message(request).await?;
                                if let Err(err) = self
                                    .swarm
                                    .behaviour_mut()
                                    .request_response
                                    .send_response(channel, RbMsgResponse::new(rb_id))
                                {
                                    log::error!("Error sending response: {:?}", err);
                                }
                            }
                            request_response::Message::Response {
                                request_id,
                                response,
                            } => {
                                log::trace!("Received response {response:?} from peer: {peer:?}, request_id: {request_id:?}",);
                            }
                        },
                        request_response::Event::OutboundFailure {
                            peer,
                            request_id,
                            error,
                        } => {
                            log::error!("Outbound failure: {error:?}, peer:{peer:?}, request_id:{request_id:?}",);
                        }
                        request_response::Event::InboundFailure {
                            peer,
                            request_id,
                            error,
                        } => {
                            log::error!("Inbound failure: {error:?}, peer:{peer:?}, request_id:{request_id:?}",);
                        }
                        request_response::Event::ResponseSent { peer, request_id } => {
                            log::trace!("Response sent to peer: {peer:?}, {request_id:?}",);
                        }
                    }
                }
                _ => {}
            },
            //Ignore other Swarm events for now
            _ => {}
        }
        Ok(())
    }

    async fn send_protocol_message(&mut self, msg: RbMsg) {
        log::debug!("Sending Block message: {}", msg.id);
        for peer in self.swarm.behaviour_mut().peer_discovery.peer_ids() {
            log::trace!("Sending Block message to peer: {:?}", peer);
            self.swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer, msg.clone());
        }
    }

    async fn send_ephemera_message(&mut self, msg: EphemeraMessage, topic: &Topic) {
        log::debug!("Sending Ephemera message: {}", msg.id);
        self.send_message(msg, topic).await;
    }

    async fn send_message<T: serde::Serialize>(&mut self, msg: T, topic: &Topic) {
        match serde_json::to_vec(&msg) {
            Ok(vec) => {
                if let Err(err) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic.clone(), vec)
                {
                    log::error!("Error publishing message: {}", err);
                }
            }
            Err(err) => {
                log::error!("Error serializing message: {}", err);
            }
        }
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "GroupBehaviourEvent")]
struct GroupNetworkBehaviour {
    gossipsub: gossipsub::Behaviour,
    peer_discovery: StaticPeerDiscovery,
    request_response: request_response::Behaviour<RbMsgMessagesCodec>,
}

#[allow(clippy::large_enum_variant)]
enum GroupBehaviourEvent {
    Gossipsub(gossipsub::Event),
    RequestResponse(request_response::Event<RbMsg, RbMsgResponse>),
    StaticPeerDiscovery(()),
}

impl From<gossipsub::Event> for GroupBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        GroupBehaviourEvent::Gossipsub(event)
    }
}

impl From<()> for GroupBehaviourEvent {
    fn from(event: ()) -> Self {
        GroupBehaviourEvent::StaticPeerDiscovery(event)
    }
}

impl From<request_response::Event<RbMsg, RbMsgResponse>> for GroupBehaviourEvent {
    fn from(event: request_response::Event<RbMsg, RbMsgResponse>) -> Self {
        GroupBehaviourEvent::RequestResponse(event)
    }
}

//Create combined behaviour.
//Gossipsub takes care of message delivery semantics
//Peer discovery takes care of locating peers
fn create_behaviour(
    libp2p_conf: &Libp2pConfig,
    keypair: Arc<Ed25519Keypair>,
) -> GroupNetworkBehaviour {
    let consensus_topic = Topic::new(&libp2p_conf.consensus_msg_topic_name);
    let proposed_topic = Topic::new(&libp2p_conf.proposed_msg_topic_name);

    let mut gossipsub = create_gossipsub(keypair);
    gossipsub.subscribe(&consensus_topic).unwrap();
    gossipsub.subscribe(&proposed_topic).unwrap();

    let peer_discovery = StaticPeerDiscovery::new(libp2p_conf);
    for peer in peer_discovery.peer_ids() {
        log::debug!("Adding peer: {}", peer);
        gossipsub.add_explicit_peer(&peer);
    }

    let request_response = create_request_response();

    GroupNetworkBehaviour {
        gossipsub,
        peer_discovery,
        request_response,
    }
}

//Configure networking messaging stack(Gossipsub)
fn create_gossipsub(local_key: Arc<Ed25519Keypair>) -> gossipsub::Behaviour {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(5))
        .validation_mode(ValidationMode::Strict)
        .build()
        .expect("Valid config");

    gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(local_key.0.clone()),
        gossipsub_config,
    )
    .expect("Correct configuration")
}

fn create_request_response() -> request_response::Behaviour<RbMsgMessagesCodec> {
    request_response::Behaviour::new(
        RbMsgMessagesCodec,
        iter::once((RbMsgProtocol, request_response::ProtocolSupport::Full)),
        Default::default(),
    )
}

//Configure networking connection stack(Tcp, Noise, Yamux)
//Tcp protocol for networking
//Noise protocol for encryption
//Yamux protocol for multiplexing
fn create_transport(local_key: Arc<Ed25519Keypair>) -> Boxed<(Libp2pPeerId, StreamMuxerBox)> {
    let transport = TokioTransport::new(TokioConfig::default().nodelay(true));
    let noise_keypair = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&local_key.0.clone())
        .unwrap();
    let xx_config = noise::NoiseConfig::xx(noise_keypair);
    transport
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(xx_config.into_authenticated())
        .multiplex(YamuxConfig::default())
        .timeout(Duration::from_secs(20))
        .boxed()
}

#[derive(Clone)]
struct RbMsgMessagesCodec;

impl RbMsgMessagesCodec {
    async fn write_length_prefixed<D: AsRef<[u8]>, I: AsyncWrite + Unpin>(
        io: &mut I,
        data: D,
    ) -> Result<(), std::io::Error> {
        Self::write_varint(io, data.as_ref().len() as u32).await?;
        io.write_all(data.as_ref()).await?;
        io.flush().await?;

        Ok(())
    }

    async fn write_varint<I: AsyncWrite + Unpin>(
        io: &mut I,
        len: u32,
    ) -> Result<(), std::io::Error> {
        let mut len_data = unsigned_varint::encode::u32_buffer();
        let encoded_len = unsigned_varint::encode::u32(len, &mut len_data).len();
        io.write_all(&len_data[..encoded_len]).await?;

        Ok(())
    }

    async fn read_varint<T: AsyncRead + Unpin>(io: &mut T) -> Result<u32, std::io::Error> {
        let mut buffer = unsigned_varint::encode::u32_buffer();
        let mut buffer_len = 0;

        loop {
            //read 1 byte at time because we don't know how it compacted 32 bit integer
            io.read_exact(&mut buffer[buffer_len..buffer_len + 1])
                .await?;
            buffer_len += 1;
            match unsigned_varint::decode::u32(&buffer[..buffer_len]) {
                Ok((len, _)) => {
                    log::trace!("Read varint: {}", len);
                    return Ok(len);
                }
                Err(unsigned_varint::decode::Error::Overflow) => {
                    log::error!("Invalid varint received");
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid varint",
                    ));
                }
                Err(unsigned_varint::decode::Error::Insufficient) => {
                    continue;
                }
                Err(_) => {
                    log::error!("Varint decoding error: #[non_exhaustive]");
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid varint",
                    ));
                }
            }
        }
    }

    async fn read_length_prefixed<T: AsyncRead + Unpin>(
        io: &mut T,
        max_size: u32,
    ) -> Result<Vec<u8>, std::io::Error> {
        let len = Self::read_varint(io).await?;
        if len > max_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Message too large",
            ));
        }

        let mut buf = vec![0; len as usize];
        io.read_exact(&mut buf).await?;
        Ok(buf)
    }
}

#[derive(Clone)]
struct RbMsgProtocol;

impl request_response::ProtocolName for RbMsgProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/ephemera-rbmsg/1".as_bytes()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RbMsgResponse {
    id: String,
}

impl RbMsgResponse {
    fn new(id: String) -> Self {
        Self { id }
    }
}

#[async_trait]
impl request_response::Codec for RbMsgMessagesCodec {
    type Protocol = RbMsgProtocol;
    type Request = RbMsg;
    type Response = RbMsgResponse;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> Result<Self::Request, std::io::Error>
    where
        T: AsyncRead + Unpin + Send,
    {
        let data = Self::read_length_prefixed(io, 1024 * 1024).await?;
        let msg = serde_json::from_slice(&data)?;
        log::trace!("Received request {:?}", msg);
        Ok(msg)
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let response = Self::read_length_prefixed(io, 1024 * 1024).await?;
        let response = serde_json::from_slice(&response)?;
        log::trace!("Received response {:?}", response);
        Ok(response)
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> Result<(), std::io::Error>
    where
        T: AsyncWrite + Unpin + Send,
    {
        log::trace!("Writing request {:?}", req);
        let data = serde_json::to_vec(&req).unwrap();
        Self::write_length_prefixed(io, data).await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        response: Self::Response,
    ) -> std::io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        log::trace!("Writing response {:?}", response);
        let response = serde_json::to_vec(&response).unwrap();
        Self::write_length_prefixed(io, response).await?;
        Ok(())
    }
}
