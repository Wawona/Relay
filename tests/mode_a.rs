//! Technical artifact checks, not App Store approval or a complete API audit.
#![cfg(feature = "wasmtime-baseline")]
use object::{Object, ObjectSection};

#[test]
fn translated_guest_is_pulley_data_without_executable_text() -> wasmtime::Result<()> {
    let engine = wwn_runtime::baseline::engine()?;
    let module = wasmtime::Module::new(&engine,
        b"\0asm\x01\0\0\0\x01\x04\x01\x60\0\0\x03\x02\x01\0\x07\x0a\x01\x06_start\0\0\x0a\x04\x01\x02\0\x0b")?;
    let bytes = module.serialize()?;
    let file = object::File::parse(bytes.as_slice())?;
    // Pinned Wasmtime 33 obj.rs: OSABI 200, MODULE=1, PULLEY64=8.
    match file.flags() {
        object::FileFlags::Elf {
            os_abi: 200,
            e_flags,
            ..
        } => assert_eq!(e_flags & 9, 9),
        flags => panic!("unexpected translated artifact: {flags:?}"),
    }
    let text = file
        .section_by_name(".text")
        .expect("translated bytecode section");
    match text.flags() {
        // Wasmtime marks Pulley text SH_WASMTIME_NOT_EXECUTED=1; ELF EXEC=4.
        object::SectionFlags::Elf { sh_flags } => {
            assert_eq!(sh_flags & 1, 1);
            assert_eq!(sh_flags & object::elf::SHF_EXECINSTR as u64, 0);
        }
        flags => panic!("unexpected text flags: {flags:?}"),
    }
    Ok(())
}
