#!/usr/bin/env python3
"""Writes the SBOM of a dist-toolchain archive: an SPDX 2.3 JSON document
(--out) and the same as a CycloneDX 1.6 JSON BOM (--cyclonedx).

  sbom.py --out FILE [--cyclonedx FILE] --name NAME --root ROOT
          --rx-rust COMMIT --rust COMMIT --llvm COMMIT --qemu COMMIT
          --qemu-version TEXT --rustc-version TEXT --cargo-version TEXT
          [--libxml2 VERSION] [--libs FILE --libs-kind deb|msys2
          [--distro ID]]

scripts/dist-toolchain.sh and scripts/dist-toolchain-windows.sh call it. It
lists what the archive holds:
- the forks it is built from: rust (rustc, its std for the host, the library
  sources for -Zbuild-std, rust-lldb), llvm-project (rustc's LLVM, lld, the
  LLVM tools and LLDB) and qemu (qemu-system-rx), plus rx-rust, the scripts
  that build it;
- the stage0 cargo, a binary of the Rust project;
- libxml2, which LLDB links statically, when the archive has LLDB;
- the shared libraries that qemu-system-rx needs and the archive brings
  (--libs: lines of "file<TAB>package<TAB>version"; package and version may
  be empty), as Debian (Ubuntu) or MSYS2 packages;
- the third-party crates compiled into rustc and into the host std: the
  dependencies of rustc-main in rust/Cargo.lock and of sysroot in
  rust/library/Cargo.lock, as Cargo.lock resolves them (for every platform,
  so a few are not linked on this host). Cargo.lock has no licenses, so the
  crates' licenses are NOASSERTION; their purls and checksums identify them.
"""
import argparse
import datetime
import json
import os
import re
import uuid
from urllib.parse import quote

ORG = "Organization: CDEF-inc"
RUST_PROJECT = "Organization: The Rust Project"

# The upstream projects' licenses of the bundled shared libraries, by file
# name. They are the licenses the projects declare, not checked against each
# binary package; anything else is NOASSERTION.
LIB_LICENSES = [
    (r"^lib(blkid|mount)\b", "LGPL-2.1-or-later"),
    (r"^libbz2\b", "bzip2-1.0.6"),
    (r"^libfdt\b", "GPL-2.0-or-later OR BSD-2-Clause"),
    (r"^libffi\b", "MIT"),
    (r"^libgcrypt\b", "LGPL-2.1-or-later"),
    (r"^lib(glib|gio|gmodule|gobject)-2\.0\b", "LGPL-2.1-or-later"),
    (r"^libgpg-error\b", "LGPL-2.1-or-later"),
    (r"^libjpeg\b", "IJG AND BSD-3-Clause AND Zlib"),
    (r"^lib(ncursesw|tinfo)\b", "X11-distribute-modifications-variant"),
    (r"^libpcre2-8\b", "BSD-3-Clause WITH PCRE2-exception"),
    (r"^libpcre\.", "BSD-3-Clause"),
    (r"^libpixman-1\b", "MIT"),
    (r"^libpng16\b", "libpng-2.0"),
    (r"^libzstd\b", "BSD-3-Clause OR GPL-2.0-only"),
    (r"^lib(iconv|intl)\b", "LGPL-2.1-or-later"),
    (r"^libwinpthread\b", "MIT AND BSD-3-Clause"),
    (r"^zlib1\b", "Zlib"),
]


def lib_license(files):
    found = set()
    for f in files:
        for pat, lic in LIB_LICENSES:
            if re.search(pat, f):
                found.add(lic)
                break
        else:
            return "NOASSERTION"
    return found.pop() if len(found) == 1 else "NOASSERTION"


def spdx_id(*parts):
    return "SPDXRef-" + "-".join(re.sub(r"[^A-Za-z0-9.]+", ".", p) for p in parts)


def parse_lock(path):
    """The [[package]] entries of a Cargo.lock: name, version, source,
    checksum and dependencies."""
    pkgs, cur, in_deps = [], None, False
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if line == "[[package]]":
                cur = {"deps": []}
                pkgs.append(cur)
                in_deps = False
            elif cur is None:
                continue
            elif in_deps:
                if line.strip() == "]":
                    in_deps = False
                else:
                    m = re.match(r'\s*"(.*)",?$', line)
                    if m:
                        cur["deps"].append(m.group(1))
            elif line.startswith("dependencies = ["):
                in_deps = not line.endswith("]")
                cur["deps"] += re.findall(r'"([^"]*)"', line)
            else:
                m = re.match(r'(\w+) = "(.*)"$', line)
                if m:
                    cur[m.group(1)] = m.group(2)
    return pkgs


def closure(pkgs, roots):
    """The packages that the roots depend on, transitively (roots included).
    A dependency is "name", "name version" or "name version (source)"."""
    def find(dep):
        parts = dep.split(" ", 2)
        c = [p for p in pkgs if p["name"] == parts[0]]
        if len(parts) > 1:
            c = [p for p in c if p["version"] == parts[1]]
        if len(parts) > 2:
            c = [p for p in c if p.get("source") == parts[2].strip("()")]
        if len(c) != 1:
            raise SystemExit(f"Cargo.lock: cannot resolve dependency {dep!r}")
        return c[0]

    seen, todo = {}, [find(r) for r in roots]
    while todo:
        p = todo.pop()
        key = (p["name"], p["version"], p.get("source"))
        if key in seen:
            continue
        seen[key] = p
        todo += [find(d) for d in p["deps"]]
    return list(seen.values())


def package(name, spdxid, version, supplier, download, license_declared,
            purl=None, comment=None, checksum=None, purpose=None):
    p = {
        "name": name,
        "SPDXID": spdxid,
        "versionInfo": version or "NOASSERTION",
        "supplier": supplier,
        "downloadLocation": download,
        "filesAnalyzed": False,
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": license_declared,
        "copyrightText": "NOASSERTION",
    }
    if purpose:
        p["primaryPackagePurpose"] = purpose
    if checksum:
        p["checksums"] = [{"algorithm": "SHA256", "checksumValue": checksum}]
    if purl:
        p["externalRefs"] = [{"referenceCategory": "PACKAGE-MANAGER",
                              "referenceType": "purl", "referenceLocator": purl}]
    if comment:
        p["comment"] = comment
    return p


def cyclonedx(doc):
    """The SPDX document `doc` as a CycloneDX 1.6 BOM: the archive is the
    metadata's component, every other package a component (bom-ref = its
    SPDX id), and the DEPENDS_ON relationships (and what the archive
    contains directly) its dependencies."""
    top = doc["documentDescribes"][0]
    pkgs = {p["SPDXID"]: p for p in doc["packages"]}

    def component(p):
        sid = p["SPDXID"]
        library = re.match(r"SPDXRef-Package-(crate|deb|msys2|lib|libxml2)\b", sid)
        c = {"type": "library" if library else "application",
             "bom-ref": sid, "name": p["name"]}
        if p["versionInfo"] != "NOASSERTION":
            c["version"] = p["versionInfo"]
        if p["supplier"].startswith("Organization: "):
            c["supplier"] = {"name": p["supplier"][len("Organization: "):]}
        if p.get("comment"):
            c["description"] = p["comment"]
        if p["licenseDeclared"] != "NOASSERTION":
            c["licenses"] = [{"expression": p["licenseDeclared"]}]
        for ck in p.get("checksums", []):
            c.setdefault("hashes", []).append(
                {"alg": "SHA-256", "content": ck["checksumValue"]})
        for r in p.get("externalRefs", []):
            if r["referenceType"] == "purl":
                c["purl"] = r["referenceLocator"]
        dl = p["downloadLocation"]
        if dl != "NOASSERTION":
            vcs = dl.startswith("git+")
            if vcs:  # git+URL@REV -> URL#REV
                url, _, rev = dl[4:].rpartition("@")
                dl = f"{url}#{rev}"
            c["externalReferences"] = [{"type": "vcs" if vcs else "distribution",
                                        "url": dl}]
        return c

    deps = {}
    for r in doc["relationships"]:
        a, t, b = r["spdxElementId"], r["relationshipType"], r["relatedSpdxElement"]
        if t == "DEPENDS_ON" or (t == "CONTAINS" and a == top):
            deps.setdefault(a, []).append(b)
    # What the archive contains through a dependency is not listed again.
    nested = {b for a, bs in deps.items() if a != top for b in bs}
    if top in deps:
        deps[top] = [b for b in deps[top] if b not in nested]
    meta = component(pkgs[top])
    meta["type"] = "application"
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{uuid.uuid4()}",
        "version": 1,
        "metadata": {
            "timestamp": doc["creationInfo"]["created"],
            "tools": {"components": [{"type": "application", "name": "rx-rust scripts/sbom.py"}]},
            "component": meta,
            "supplier": {"name": ORG[len("Organization: "):]},
        },
        "components": [component(p) for sid, p in pkgs.items() if sid != top],
        "dependencies": [{"ref": sid, "dependsOn": deps.get(sid, [])} for sid in pkgs],
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    for a in ("out", "name", "root", "rx-rust", "rust", "llvm", "qemu",
              "qemu-version", "rustc-version", "cargo-version"):
        ap.add_argument("--" + a, required=True)
    ap.add_argument("--cyclonedx", help="also write a CycloneDX 1.6 JSON BOM here")
    ap.add_argument("--libxml2")
    ap.add_argument("--libs")
    ap.add_argument("--libs-kind", choices=("deb", "msys2"))
    ap.add_argument("--distro", help="e.g. ubuntu-22.04 (for deb purls)")
    a = ap.parse_args()

    gh = "https://github.com/CDEF-inc"
    pkgs, rels = [], []
    top = spdx_id("Package", a.name)
    pkgs.append(package(
        a.name, top, a.rx_rust, ORG, "NOASSERTION", "NOASSERTION",
        purpose="ARCHIVE",
        comment="The prebuilt RX toolchain archive (rx-rust's dist-toolchain "
                "scripts). Each component keeps its own license."))

    def add(p, rel="CONTAINS", parent=None):
        pkgs.append(p)
        rels.append({"spdxElementId": parent or top, "relationshipType": rel,
                     "relatedSpdxElement": p["SPDXID"]})
        return p["SPDXID"]

    add(package("rx-rust", spdx_id("Package", "rx-rust"), a.rx_rust, ORG,
                f"git+{gh}/rx-rust.git@{a.rx_rust}", "MIT OR Apache-2.0",
                purl=f"pkg:github/CDEF-inc/rx-rust@{a.rx_rust}",
                comment="The scripts that build and pack the toolchain."),
        rel="GENERATED_FROM")
    rust = add(package(
        "rust", spdx_id("Package", "rust"), a.rust, ORG,
        f"git+{gh}/rust.git@{a.rust}", "MIT OR Apache-2.0",
        purl=f"pkg:github/CDEF-inc/rust@{a.rust}",
        comment=f"The Rust fork ({a.rustc_version}): rustc, librustc_driver, "
                "the host's std, library sources for -Zbuild-std, rust-lldb "
                "and the LLDB formatters."))
    llvm = add(package(
        "llvm-project", spdx_id("Package", "llvm-project"), a.llvm, ORG,
        f"git+{gh}/llvm-project.git@{a.llvm}",
        "Apache-2.0 WITH LLVM-exception",
        purl=f"pkg:github/CDEF-inc/llvm-project@{a.llvm}",
        comment="The LLVM fork with the RX backend: rustc's LLVM, lld "
                "(rust-lld, ld.lld), the LLVM binary tools and LLDB."))
    qemu = add(package(
        "qemu", spdx_id("Package", "qemu"), a.qemu, ORG,
        f"git+{gh}/qemu.git@{a.qemu}", "GPL-2.0-only",
        purl=f"pkg:github/CDEF-inc/qemu@{a.qemu}",
        comment=f"The QEMU fork with RXv2 / RXv3 / RXv3e ({a.qemu_version}): "
                "qemu-system-rx."))
    m = re.match(r"cargo (\S+)", a.cargo_version)
    add(package(
        "cargo", spdx_id("Package", "cargo"), m.group(1) if m else None,
        RUST_PROJECT, "https://static.rust-lang.org/dist/", "MIT OR Apache-2.0",
        purl=f"pkg:generic/cargo@{m.group(1)}" if m else None,
        comment=f"The stage0 cargo, the Rust project's binary ({a.cargo_version})."))
    if a.libxml2:
        v = a.libxml2
        add(package(
            "libxml2", spdx_id("Package", "libxml2"), v, "Organization: GNOME",
            f"https://download.gnome.org/sources/libxml2/{v.rsplit('.', 1)[0]}/libxml2-{v}.tar.xz",
            "MIT", purl=f"pkg:generic/libxml2@{v}",
            comment="Linked statically into LLDB (liblldb)."))
        rels.append({"spdxElementId": llvm, "relationshipType": "DEPENDS_ON",
                     "relatedSpdxElement": spdx_id("Package", "libxml2")})

    # The shared libraries qemu-system-rx needs, by package.
    if a.libs:
        groups = {}
        with open(a.libs, encoding="utf-8") as f:
            for line in f:
                if not line.strip():
                    continue
                file, pkg, ver = (line.rstrip("\n").split("\t") + ["", ""])[:3]
                groups.setdefault((pkg or file, ver), []).append(file)
        for (pkg, ver), files in sorted(groups.items()):
            known = pkg not in files
            if a.libs_kind == "deb":
                supplier = "Organization: Ubuntu" if (a.distro or "").startswith("ubuntu") else "NOASSERTION"
                distro = (a.distro or "").split("-")[0] or "debian"
                purl = (f"pkg:deb/{distro}/{pkg}@{quote(ver)}?arch=amd64"
                        + (f"&distro={a.distro}" if a.distro else "")) if known and ver else None
                origin = f"the {a.distro or 'Debian'} package {pkg}"
            else:
                supplier = "Organization: MSYS2"
                purl = f"pkg:alpm/msys2/{pkg}@{quote(ver)}" if known and ver else None
                origin = f"the MSYS2 package {pkg}" if known else "MSYS2 (UCRT64); the package is not recorded"
            sid = add(package(
                pkg, spdx_id("Package", a.libs_kind or "lib", pkg), ver or None,
                supplier, "NOASSERTION", lib_license(files), purl=purl,
                comment=f"{', '.join(files)}: shared libraries of qemu-system-rx, "
                        f"from {origin}."))
            rels.append({"spdxElementId": qemu, "relationshipType": "DEPENDS_ON",
                         "relatedSpdxElement": sid})

    # Third-party crates in rustc and in the host std.
    done = {}
    for lock, roots, what in (
            (os.path.join(a.root, "rust", "Cargo.lock"), ["rustc-main"], "rustc"),
            (os.path.join(a.root, "rust", "library", "Cargo.lock"), ["sysroot"],
             "the host's std")):
        for p in closure(parse_lock(lock), roots):
            src = p.get("source")
            if not src:
                continue  # a crate of the rust repository itself
            key = (p["name"], p["version"], src)
            if key in done:
                done[key]["comment"] = done[key]["comment"].replace(
                    "Compiled into", "Compiled into " + what + " and")
                continue
            n, v = p["name"], p["version"]
            if src.startswith("registry+"):
                download = f"https://crates.io/api/v1/crates/{n}/{v}/download"
                purl = f"pkg:cargo/{n}@{v}"
            elif src.startswith("git+"):
                url, _, rev = src[4:].partition("#")
                url = url.split("?")[0]
                download = f"git+{url}@{rev}"
                g = re.match(r"https://github\.com/([^/]+)/([^/.]+)", url)
                purl = f"pkg:github/{g.group(1)}/{g.group(2)}@{rev}" if g else None
            else:
                download, purl = "NOASSERTION", None
            # The same name and version can come from crates.io and from git.
            sid = spdx_id("Package", "crate", n, v,
                          *([] if src.startswith("registry+") else ["git"]))
            cp = package(n, sid, v, "NOASSERTION",
                         download, "NOASSERTION", purl=purl,
                         checksum=p.get("checksum"),
                         comment=f"Compiled into {what} (Cargo.lock: {src}).")
            done[key] = cp
            add(cp)
            rels.append({"spdxElementId": rust, "relationshipType": "DEPENDS_ON",
                         "relatedSpdxElement": cp["SPDXID"]})

    doc = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": a.name,
        "documentNamespace": f"{gh}/rx-rust/sbom/{a.name}-{a.rx_rust}-{uuid.uuid4()}",
        "creationInfo": {
            "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "creators": ["Tool: rx-rust-scripts-sbom.py", ORG],
        },
        "documentDescribes": [top],
        "packages": pkgs,
        "relationships": [{"spdxElementId": "SPDXRef-DOCUMENT",
                           "relationshipType": "DESCRIBES",
                           "relatedSpdxElement": top}] + rels,
    }
    ids = [p["SPDXID"] for p in pkgs]
    dup = {i for i in ids if ids.count(i) > 1}
    if dup:
        raise SystemExit(f"duplicate SPDX ids: {sorted(dup)}")
    with open(a.out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"{a.out}: {len(pkgs)} packages")
    if a.cyclonedx:
        bom = cyclonedx(doc)
        with open(a.cyclonedx, "w", encoding="utf-8", newline="\n") as f:
            json.dump(bom, f, indent=1)
            f.write("\n")
        print(f"{a.cyclonedx}: {len(bom['components'])} components")


if __name__ == "__main__":
    main()
