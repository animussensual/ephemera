use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::block::{Block, RawBlock};
use crate::config::configuration::DbConfig;
use crate::utilities::crypto::Signature;

pub(crate) struct DbQuery {
    pub(crate) connection: Connection,
}

impl DbQuery {
    pub(crate) fn open(db_conf: DbConfig) -> Self {
        match Connection::open(db_conf.sqlite_path) {
            Ok(connection) => Self { connection },
            Err(err) => {
                panic!("Error opening database: {err}",);
            }
        }
    }

    pub(crate) fn get_block_by_id(&self, block_id: String) -> anyhow::Result<Option<Block>> {
        log::debug!("Getting block by id: {}", block_id);

        let mut stmt = self
            .connection
            .prepare_cached("SELECT block FROM blocks WHERE block_id = ?1")?;
        let block = stmt
            .query_row(params![block_id], Self::map_block())
            .optional()?;

        if let Some(block) = &block {
            log::debug!("Found block: {}", block.header);
        } else {
            log::debug!("Block not found: {}", block_id);
        };

        Ok(block.map(|b| b.into()))
    }

    pub(crate) fn get_last_block(&self) -> anyhow::Result<Option<Block>> {
        log::debug!("Getting last block");

        let mut stmt = self
            .connection
            .prepare_cached("SELECT block FROM blocks where id = (select max(id) from blocks)")?;

        let block = stmt.query_row(params![], Self::map_block()).optional()?;

        if let Some(block) = &block {
            log::debug!("Found last block: {}", block.header);
        } else {
            log::debug!("Last block not found");
        };

        Ok(block.map(|b| b.into()))
    }

    pub(crate) fn get_block_signatures(
        &self,
        block_id: String,
    ) -> anyhow::Result<Option<Vec<Signature>>> {
        log::debug!("Getting block signatures by block id {}", block_id);

        let mut stmt = self
            .connection
            .prepare_cached("SELECT signatures FROM signatures where block_id = ?1")?;

        let signatures = stmt
            .query_row(params![block_id], |row| {
                let signatures: Vec<u8> = row.get(0)?;
                let signatures =
                    serde_json::from_slice::<Vec<Signature>>(&signatures).map_err(|e| {
                        log::error!("Error deserializing block: {}", e);
                        rusqlite::Error::InvalidQuery {}
                    })?;
                Ok(signatures)
            })
            .optional()?;

        if signatures.is_some() {
            log::debug!("Found block {} signatures", block_id);
        } else {
            log::debug!("Signatures not found");
        };

        Ok(signatures)
    }

    fn map_block() -> impl FnOnce(&Row) -> Result<RawBlock, rusqlite::Error> {
        |row| {
            let body: Vec<u8> = row.get(0)?;
            let block = serde_json::from_slice::<RawBlock>(&body).map_err(|e| {
                log::error!("Error deserializing block: {}", e);
                rusqlite::Error::InvalidQuery {}
            })?;
            Ok(block)
        }
    }
}