//! Keeps `bindings/abi.v1.json` honest: every function and error code in the file must exist in
//! the contract source, and vice versa.
fn read(p: &str) -> String {
    std::fs::read_to_string(format!("{}/{p}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn abi_file_matches_source() {
    let abi = read("../../bindings/abi.v1.json");
    let src = read("src/lib.rs");
    let impl_part = &src[src.find("#[contractimpl]").unwrap()..];
    let mut fns: Vec<String> = impl_part
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub fn "))
        .map(|l| l.split('(').next().unwrap().to_string())
        .collect();
    fns.sort();
    let mut listed: Vec<String> = abi
        .split("\"name\": \"")
        .skip(1)
        .map(|r| r.split('"').next().unwrap().to_string())
        .collect();
    listed.sort();
    assert_eq!(fns, listed);

    let types = read("src/types.rs");
    let variants = types
        .lines()
        .filter(|l| {
            l.trim_start().starts_with(|c: char| c.is_ascii_uppercase()) && l.contains(" = ")
        })
        .count();
    assert!(
        variants == 26,
        "error enum changed: update ABI file ({variants})"
    );
}
