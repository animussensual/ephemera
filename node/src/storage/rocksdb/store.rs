use std::sync::Arc;

use crate::block::types::block::Block;
use crate::storage::rocksdb::{block_hash_key, block_height_key, certificates_key, last_block_key};
use rocksdb::{TransactionDB, WriteBatchWithTransaction};

use crate::utilities::crypto::Certificate;

pub struct DbStore {
    connection: Arc<TransactionDB>,
}

impl DbStore {
    pub fn new(db: Arc<TransactionDB>) -> DbStore {
        DbStore { connection: db }
    }

    pub(crate) fn store_block(
        &self,
        block: &Block,
        certificates: Vec<Certificate>,
    ) -> anyhow::Result<()> {
        log::debug!("Storing block: {}", block.header);
        log::trace!("Storing block certificates: {}", certificates.len());

        let hash_str = block.header.hash.to_string();

        let block_id_key = block_hash_key(&hash_str);
        let certificates_key = certificates_key(&hash_str);
        let height_key = block_height_key(&block.header.height);

        // Check UNIQUE constraints
        let existing_id = self.connection.get(&block_id_key)?;
        if existing_id.is_some() {
            return Err(anyhow::anyhow!("Block already exists"));
        }

        let mut batch = WriteBatchWithTransaction::<true>::default();

        //Store last block id(without prefix!)
        //May want to check that height is incremented by 1
        batch.put(last_block_key(), hash_str.clone());

        // Store block height
        batch.put(height_key.as_bytes(), hash_str);

        // Store block(without signature)
        let block_bytes = serde_json::to_vec::<Block>(block)?;
        batch.put(block_id_key.as_bytes(), block_bytes);

        // Store block signatures
        let certificate_bytes = serde_json::to_vec(&certificates)?;
        batch.put(certificates_key.as_bytes(), certificate_bytes);

        self.connection.write(batch)?;
        Ok(())
    }
}
