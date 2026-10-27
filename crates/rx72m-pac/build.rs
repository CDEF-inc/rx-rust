//! Generates the PAC from e2 studio's RX72M.sfrx (not redistributed here).
//!
//! The file is taken from `RX72M_SFRX` if set, else from e2 studio's support
//! files under `~/.eclipse/com.renesas.platform_*/DebugComp/RX/IoFiles`
//! (also looked for under `/mnt/c/Users/*` when building in WSL).
//!
//! sfrx lists modules (some with nested modules, such as CAN0/MB0) and their
//! registers by absolute address, with bit fields. Each top-level module
//! becomes an SVD peripheral based at its lowest register address; nested
//! modules' registers (already prefixed, MB0_ID) go into their top-level
//! module. Registers at the same address (CAN's MCTLn_TX / MCTLn_RX) are
//! marked as alternates of the first one.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RX72M_SFRX");
    let sfrx = find_sfrx().unwrap_or_else(|| {
        panic!(
            "rx72m-pac: RX72M.sfrx not found. Install Renesas e2 studio with RX \
             support (it puts the file in \
             ~/.eclipse/com.renesas.platform_*/DebugComp/RX/IoFiles) or set \
             RX72M_SFRX to its path."
        )
    });
    println!("cargo:rerun-if-changed={}", sfrx.display());

    let text = fs::read_to_string(&sfrx)
        .unwrap_or_else(|e| panic!("rx72m-pac: cannot read {}: {e}", sfrx.display()));
    let svd = sfrx_to_svd(text.trim_start_matches('\u{feff}'));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("rx72m.svd"), &svd).unwrap();

    let mut config = svd2rust::Config::default();
    config.target = svd2rust::Target::None;
    config.skip_crate_attributes = true;
    let generation = svd2rust::generate(&svd, &config)
        .unwrap_or_else(|e| panic!("rx72m-pac: svd2rust failed: {e:?}"));
    fs::write(out.join("pac.rs"), generation.lib_rs).unwrap();
}

fn find_sfrx() -> Option<PathBuf> {
    if let Some(p) = env::var_os("RX72M_SFRX") {
        return Some(PathBuf::from(p));
    }
    let mut homes = Vec::new();
    for var in ["HOME", "USERPROFILE"] {
        if let Some(h) = env::var_os(var) {
            homes.push(PathBuf::from(h));
        }
    }
    if let Ok(users) = fs::read_dir("/mnt/c/Users") {
        homes.extend(users.flatten().map(|e| e.path()));
    }
    for home in homes {
        let Ok(entries) = fs::read_dir(home.join(".eclipse")) else {
            continue;
        };
        for e in entries.flatten() {
            if !e.file_name().to_string_lossy().starts_with("com.renesas.platform_") {
                continue;
            }
            let p = e.path().join("DebugComp/RX/IoFiles/RX72M.sfrx");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

struct Field {
    name: String,
    bit: u32,
    width: u32,
}

struct Register {
    name: String,
    address: u64,
    bits: u32,
    fields: Vec<Field>,
}

fn sfrx_to_svd(text: &str) -> String {
    let doc = roxmltree::Document::parse(text).expect("rx72m-pac: bad sfrx XML");
    let table = doc
        .descendants()
        .find(|n| n.has_tag_name("moduletable"))
        .expect("rx72m-pac: no <moduletable> in the sfrx file");

    let mut svd = String::new();
    svd.push_str(concat!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
        "<device schemaVersion=\"1.1\">\n",
        "<name>RX72M</name>\n<version>1.0</version>\n",
        "<description>Renesas RX72M (from e2 studio's RX72M.sfrx)</description>\n",
        "<addressUnitBits>8</addressUnitBits>\n<width>32</width>\n",
        "<size>32</size>\n<access>read-write</access>\n",
        "<resetValue>0</resetValue>\n<resetMask>0</resetMask>\n",
        "<peripherals>\n",
    ));
    for module in table.children().filter(|n| n.has_tag_name("module")) {
        let mut regs: Vec<Register> = module
            .descendants()
            .filter(|n| n.has_tag_name("register"))
            .map(|r| Register {
                name: r.attribute("name").unwrap().to_string(),
                address: parse_hex(r.attribute("address").unwrap()),
                bits: match r.attribute("size").unwrap() {
                    "B" => 8,
                    "W" => 16,
                    "LW" => 32,
                    s => panic!("rx72m-pac: unknown register size {s}"),
                },
                fields: r
                    .children()
                    .filter(|f| f.has_tag_name("bitfield"))
                    .map(|f| Field {
                        name: f.attribute("name").unwrap().to_string(),
                        bit: f.attribute("bit").unwrap().parse().unwrap(),
                        width: f.attribute("bitlength").unwrap().parse().unwrap(),
                    })
                    .collect(),
            })
            .collect();
        if regs.is_empty() {
            continue;
        }
        regs.sort_by_key(|r| r.address);
        let base = regs[0].address;
        let end = regs.iter().map(|r| r.address + r.bits as u64 / 8).max().unwrap();
        let name = module.attribute("name").unwrap();
        writeln!(
            svd,
            "<peripheral>\n<name>{name}</name>\n<baseAddress>{base:#x}</baseAddress>\n\
             <addressBlock><offset>0</offset><size>{:#x}</size><usage>registers</usage>\
             </addressBlock>\n<registers>",
            end - base
        )
        .unwrap();
        let mut first_at: BTreeMap<u64, String> = BTreeMap::new();
        for r in &regs {
            write!(svd, "<register>\n<name>{}</name>\n", r.name).unwrap();
            match first_at.get(&r.address) {
                Some(first) => writeln!(svd, "<alternateRegister>{first}</alternateRegister>")
                    .unwrap(),
                None => {
                    first_at.insert(r.address, r.name.clone());
                }
            }
            write!(
                svd,
                "<addressOffset>{:#x}</addressOffset>\n<size>{}</size>\n",
                r.address - base,
                r.bits
            )
            .unwrap();
            if !r.fields.is_empty() {
                svd.push_str("<fields>\n");
                for f in &r.fields {
                    writeln!(
                        svd,
                        "<field><name>{}</name><bitOffset>{}</bitOffset>\
                         <bitWidth>{}</bitWidth></field>",
                        f.name, f.bit, f.width
                    )
                    .unwrap();
                }
                svd.push_str("</fields>\n");
            }
            svd.push_str("</register>\n");
        }
        svd.push_str("</registers>\n</peripheral>\n");
    }
    svd.push_str("</peripherals>\n</device>\n");
    svd
}

fn parse_hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
        .unwrap_or_else(|_| panic!("rx72m-pac: bad address {s}"))
}
