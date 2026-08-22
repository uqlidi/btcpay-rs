//! Checking that a plugin's native dependencies will exist at runtime.
//!
//! A plugin's cdylib links whatever its crates link, and BTCPay's image is minimal. A missing
//! library surfaces as a `dlopen` failure naming nothing useful, after an operator has already
//! installed the package. Catching it at packaging time is cheap and turns an operator's
//! problem into a build error.

use std::path::Path;

/// Libraries BTCPay's image provides, so a plugin may depend on them freely.
///
/// Verified against `btcpayserver/btcpayserver:2.4.1`. Kept as one list so it can be corrected
/// when images change, rather than being assumed in several places.
///
/// Deliberately short. Anything not here is treated as a problem, which errs towards a build
/// error over a package that cannot load.
const PROVIDED_BY_BTCPAY: &[&str] = &[
    // glibc and the loader
    "libc.so.6",
    "libm.so.6",
    "libdl.so.2",
    "libpthread.so.0",
    "librt.so.1",
    "libresolv.so.2",
    "ld-linux-x86-64.so.2",
    "ld-linux-aarch64.so.1",
    // toolchain runtimes, present because the .NET runtime needs them
    "libgcc_s.so.1",
    "libstdc++.so.6",
];

/// A native library a plugin needs that BTCPay will not provide.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingLibrary {
    /// Soname as recorded in the library, e.g. `libzmq.so.5`.
    pub soname: String,
}

/// Reports native dependencies that BTCPay's image will not satisfy.
pub fn unsatisfied_dependencies(library: &Path) -> Result<Vec<MissingLibrary>, String> {
    let bytes =
        std::fs::read(library).map_err(|e| format!("could not read {}: {e}", library.display()))?;

    let needed = read_needed(&bytes)
        .ok_or_else(|| format!("{} is not an ELF shared library", library.display()))?;

    Ok(unsatisfied(needed, PROVIDED_BY_BTCPAY))
}

/// The set difference, separated out so the policy can be tested without needing a library
/// that is deliberately broken. Producing one of those means fighting the linker, which drops
/// unused dependencies.
fn unsatisfied(needed: Vec<String>, provided: &[&str]) -> Vec<MissingLibrary> {
    needed
        .into_iter()
        .filter(|soname| !provided.contains(&soname.as_str()))
        .map(|soname| MissingLibrary { soname })
        .collect()
}

/// Explains a set of missing libraries, and what to do about them.
pub fn explain(missing: &[MissingLibrary]) -> String {
    let names: Vec<&str> = missing.iter().map(|m| m.soname.as_str()).collect();

    format!(
        "this plugin needs native libraries that BTCPay Server's image does not provide: {}.\n\
         \n\
         It would install and then fail to load.\n\
         \n\
         The usual fix is to link them into the plugin instead of depending on the system copy.\n\
         Many -sys crates offer a feature for this, often called `vendored` or `bundled`; the\n\
         `zmq` crate does it by default, which is why it needs nothing here.\n\
         \n\
         If a library genuinely cannot be linked statically, it has to be shipped alongside the\n\
         plugin, which btcpay-rs does not do yet. Please open an issue describing the library.",
        names.join(", ")
    )
}

/// Reads `DT_NEEDED` entries out of an ELF shared library.
///
/// Parsed here rather than by running `readelf`, which is not always installed and would make
/// packaging depend on binutils being present.
fn read_needed(bytes: &[u8]) -> Option<Vec<String>> {
    // ELF identification: magic, then 64-bit class, then little endian. Only the tier-one
    // target shape is handled; anything else is reported as not an ELF library rather than
    // being parsed incorrectly.
    if bytes.len() < 64 || &bytes[0..4] != b"\x7fELF" || bytes[4] != 2 || bytes[5] != 1 {
        return None;
    }

    let read_u16 = |at: usize| -> Option<usize> {
        bytes
            .get(at..at + 2)
            .and_then(|s| s.try_into().ok())
            .map(|b| u16::from_le_bytes(b) as usize)
    };
    let read_u64 = |at: usize| -> Option<u64> {
        bytes
            .get(at..at + 8)
            .and_then(|s| s.try_into().ok())
            .map(u64::from_le_bytes)
    };

    let program_headers_at = read_u64(0x20)? as usize;
    let program_header_size = read_u16(0x36)?;
    let program_header_count = read_u16(0x38)?;

    // Find PT_DYNAMIC, which points at the dynamic section.
    let mut dynamic: Option<(usize, usize)> = None;
    for index in 0..program_header_count {
        let header = program_headers_at + index * program_header_size;
        let kind = bytes.get(header..header + 4)?;
        if u32::from_le_bytes(kind.try_into().ok()?) == 2 {
            let offset = read_u64(header + 0x08)? as usize;
            let size = read_u64(header + 0x20)? as usize;
            dynamic = Some((offset, size));
            break;
        }
    }
    let (dynamic_at, dynamic_size) = dynamic?;

    // Walk the dynamic entries once for the string table, then again for DT_NEEDED, since the
    // names are offsets into that table and it may appear after them.
    let mut string_table: Option<usize> = None;
    let mut needed_offsets = Vec::new();

    let mut at = dynamic_at;
    while at + 16 <= dynamic_at + dynamic_size && at + 16 <= bytes.len() {
        let tag = read_u64(at)?;
        let value = read_u64(at + 8)?;
        match tag {
            0 => break,                               // DT_NULL
            1 => needed_offsets.push(value as usize), // DT_NEEDED
            5 => string_table = Some(value as usize), // DT_STRTAB
            _ => {}
        }
        at += 16;
    }

    // DT_STRTAB is a virtual address; for a shared library the load bias is zero, so it
    // doubles as a file offset. True for anything rustc produces.
    let strings_at = string_table?;

    Some(
        needed_offsets
            .into_iter()
            .filter_map(|offset| read_c_string(bytes, strings_at + offset))
            .collect(),
    )
}

fn read_c_string(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let end = rest.iter().position(|b| *b == 0)?;
    std::str::from_utf8(&rest[..end]).ok().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The library this test binary was built alongside. Any real cdylib will do; this checks
    /// the parser against something a linker actually produced rather than a fixture.
    fn a_real_library() -> Option<std::path::PathBuf> {
        let candidates = [
            "target/release/libbtcpay_plugin_native.so",
            "target/debug/libbtcpay_plugin_native.so",
        ];
        candidates
            .iter()
            .map(|p| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(p)
            })
            .find(|p| p.exists())
    }

    #[test]
    fn a_plugin_library_needs_only_what_btcpay_provides() {
        // The example plugin must stay installable: if it grows a dependency BTCPay cannot
        // satisfy, that is a packaging bug and this is where it should surface.
        let Some(library) = a_real_library() else {
            eprintln!("skipped: no built plugin library to inspect");
            return;
        };

        let missing = unsatisfied_dependencies(&library).expect("should parse a real cdylib");

        assert!(
            missing.is_empty(),
            "the example plugin depends on libraries BTCPay does not provide: {missing:?}"
        );
    }

    #[test]
    fn the_parser_finds_the_libraries_a_linker_recorded() {
        // Guards against a parser that silently returns nothing, which would make the check
        // pass for every plugin.
        let Some(library) = a_real_library() else {
            eprintln!("skipped: no built plugin library to inspect");
            return;
        };

        let needed =
            read_needed(&std::fs::read(&library).unwrap()).expect("should parse a real cdylib");

        assert!(
            needed.iter().any(|n| n.starts_with("libc.so")),
            "every Rust cdylib links libc; got {needed:?}"
        );
    }

    #[test]
    fn a_library_the_image_lacks_is_reported() {
        let needed = vec![
            "libc.so.6".to_string(),
            "libzmq.so.5".to_string(),
            "libstdc++.so.6".to_string(),
        ];

        let missing = unsatisfied(needed, PROVIDED_BY_BTCPAY);

        assert_eq!(
            missing,
            vec![MissingLibrary {
                soname: "libzmq.so.5".into()
            }],
            "only the library BTCPay does not provide should be reported"
        );
    }

    #[test]
    fn everything_in_the_baseline_is_accepted() {
        // A false positive here would refuse to build a perfectly good plugin.
        let needed = PROVIDED_BY_BTCPAY.iter().map(|s| s.to_string()).collect();

        assert!(unsatisfied(needed, PROVIDED_BY_BTCPAY).is_empty());
    }

    #[test]
    fn something_that_is_not_a_library_is_reported_rather_than_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-elf.so");
        std::fs::write(&path, b"this is not an ELF file").unwrap();

        let err = unsatisfied_dependencies(&path).unwrap_err();

        assert!(err.contains("not an ELF"), "got: {err}");
    }

    #[test]
    fn a_missing_library_is_explained_with_a_way_out() {
        let explanation = explain(&[MissingLibrary {
            soname: "libzmq.so.5".into(),
        }]);

        assert!(explanation.contains("libzmq.so.5"), "should name it");
        assert!(
            explanation.contains("vendored") || explanation.contains("bundled"),
            "should say what to do: {explanation}"
        );
    }
}
