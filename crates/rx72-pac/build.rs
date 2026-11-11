//! Generates the PAC from e2 studio's RX72M.sfrx or RX72N.sfrx, as the
//! `rx72m` / `rx72n` feature picks (see sfrx-pac).

fn main() {
    sfrx_pac::generate_one_of(&["RX72M", "RX72N"]);
}
