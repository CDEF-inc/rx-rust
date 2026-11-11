//! Generates the PAC from e2 studio's RX74M.sfrx or RX74N.sfrx, as the
//! `rx74m` / `rx74n` feature picks (see sfrx-pac).

fn main() {
    sfrx_pac::generate_one_of(&["RX74M", "RX74N"]);
}
