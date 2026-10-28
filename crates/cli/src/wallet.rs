//! Reads a treasury wallet's own database (zcash_client_sqlite, as written by
//! zcash-devtool) through its public `v_tx_outputs` view. Read-only.
//!
//! An auditor can run the same code against a wallet they built by importing
//! the treasury's full viewing key: everything here needs viewing capability
//! only, never spending keys.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

#[derive(Clone, Debug)]
pub struct Output {
    pub txid: String,
    pub height: Option<u32>,
    /// 0 transparent, 2 Sapling, 3 Orchard (zcash_client_sqlite pool codes).
    pub pool: i64,
    pub from_us: bool,
    pub to_us: bool,
    pub to_address: Option<String>,
    pub zat: u64,
    pub is_change: bool,
    pub memo: Option<Vec<u8>>,
}

pub const POOL_TRANSPARENT: i64 = 0;

pub fn open(db: &str) -> Result<Connection> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening wallet database {db}"))
}

/// Display order txid (the stored blob is little-endian).
fn txid_hex(blob: Vec<u8>) -> String {
    let mut b = blob;
    b.reverse();
    hex::encode(b)
}

pub fn outputs(conn: &Connection) -> Result<Vec<Output>> {
    let mut st = conn.prepare(
        "SELECT txid, tx_mined_height, output_pool, from_account_uuid IS NOT NULL,
                to_account_uuid IS NOT NULL, to_address, value, is_change, memo
         FROM v_tx_outputs",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Output {
            txid: txid_hex(r.get::<_, Vec<u8>>(0)?),
            height: r.get::<_, Option<u32>>(1)?,
            pool: r.get(2)?,
            from_us: r.get(3)?,
            to_us: r.get(4)?,
            to_address: r.get(5)?,
            zat: r.get::<_, i64>(6)? as u64,
            is_change: r.get(7)?,
            memo: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

impl Output {
    /// Money that came in from outside the treasury.
    pub fn is_income(&self) -> bool {
        self.to_us && !self.from_us && !self.is_change
    }
    /// Money the treasury sent to someone else.
    pub fn is_payout(&self) -> bool {
        self.from_us && !self.to_us && !self.is_change
    }
}
