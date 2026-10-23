//! 中身から SXF を見分ける（detect_sxf）。ISO-10303-21 で始まるファイルのうち、FILE_SCHEMA の 1 つ目のスキーマが
//! SXF の ASSOCIATIVE_DRAUGHTING（STEP AP202）でないもの（IFC、STEP の AP203・AP214・AP242）は SXF ではない。
//! FILE_SCHEMA のないものは SXF とみなす。packages/reader/tests/reader.test.ts が同じ入力で TypeScript の
//! detectDrawing と比べる

mod common;

use common::{FIXTURES, read_fixture};
use zumen_sxf::detect_sxf;

/// SXF の図面が書く FILE_SCHEMA の行
const SXF_SCHEMA: &str = "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));\n";

/// ヘッダの FILE_SCHEMA の所に schema（1 バイトを 1 文字とする）を書いた、インスタンスが 1 つの STEP の物理ファイル
fn step(schema: &[u8]) -> Vec<u8> {
    let head = b"ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
        FILE_NAME('x','2026-10-23T00:00:00',(''),(''),'','','');\n";
    let tail = b"\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('',(0.,0.));\nENDSEC;\nEND-ISO-10303-21;\n";
    [head.as_slice(), schema, tail].concat()
}

/// 最初の from を to に置き換える
fn replace(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let at = bytes.windows(from.len()).position(|window| window == from.as_bytes()).expect(from);
    [&bytes[..at], to.as_bytes(), &bytes[at + from.len()..]].concat()
}

#[test]
fn ifc_and_other_step_schemas_are_not_sxf() {
    let schemas: [&[u8]; 17] = [
        b"FILE_SCHEMA(('IFC2X3'));",
        b"FILE_SCHEMA(('IFC4'));",
        b"FILE_SCHEMA(('IFC4X3_ADD2'));",
        // AP203、AP214（スキーマのオブジェクト識別子つき）、AP242
        b"FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));",
        b"FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));",
        b"FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }'));",
        // 大文字と小文字、空白（JavaScript の \s の VT・FF・NBSP も）
        b"FILE_SCHEMA(('ifc4'));",
        b"file_schema(('IFC4'));",
        b"FILE_SCHEMA ( ( 'IFC4' ) ) ;",
        b"FILE_SCHEMA\r\n\t(\t(\r\n'IFC4'));",
        b"FILE_SCHEMA\x0b\x0c\xa0(('IFC4'));",
        // 名前の一部が同じもの、\s でない U+0085 が続くもの、ß（大文字にすると SS）を含むもの
        b"FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING_X'));",
        b"FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING\x85'));",
        b"FILE_SCHEMA(('A\xdfOCIATIVE_DRAUGHTING'));",
        // 1 つ目がほかのスキーマのもの、空の名前、FILE_SCHEMA という文字列を含む FILE_NAME の後
        b"FILE_SCHEMA(('IFC4','ASSOCIATIVE_DRAUGHTING'));",
        b"FILE_SCHEMA((''));",
        b"FILE_NAME('FILE_SCHEMA');\nFILE_SCHEMA(('IFC4'));",
    ];
    for schema in schemas {
        assert_eq!(detect_sxf(&step(schema)), None, "{}", schema.escape_ascii());
    }
    // ISO-10303-21 の前の空白と改行は、これまでどおり飛ばす
    assert_eq!(detect_sxf(&[b" \r\n".as_slice(), &step(b"FILE_SCHEMA(('IFC4'));")].concat()), None);
}

#[test]
fn sxf_schema_in_any_case_with_spaces_and_an_object_identifier() {
    let schemas: [&[u8]; 10] = [
        b"FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));",
        b"FILE_SCHEMA(('associative_draughting'));",
        b"file_schema (( ' Associative_Draughting\t' ));",
        b"FILE_SCHEMA\xa0(('\xa0ASSOCIATIVE_DRAUGHTING\xa0'));",
        b"FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING { 1 0 10303 202 1 1 1 }'));",
        b"FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING{1 0 10303 202 1 1 1}'));",
        b"FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING','IFC4'));",
        // FILE_SCHEMA がない、スキーマ名の文字列がないものは、これまでどおり SXF
        b"",
        b"FILE_SCHEMA(());",
        b"FILE_SCHEMA($);",
    ];
    for schema in schemas {
        assert_eq!(detect_sxf(&step(schema)), Some("p21"), "{}", schema.escape_ascii());
    }
}

#[test]
fn sxf_without_file_schema_is_still_sxf() {
    for (format, name) in FIXTURES {
        let bytes = read_fixture(format, name);
        assert_eq!(detect_sxf(&bytes), Some(format), "{format}/{name}");
        assert_eq!(detect_sxf(&replace(&bytes, SXF_SCHEMA, "")), Some(format), "{format}/{name}");
        let ifc = replace(&bytes, SXF_SCHEMA, "FILE_SCHEMA(('IFC4'));\n");
        assert_eq!(detect_sxf(&ifc), None, "{format}/{name}");
    }
}

/// 先頭の 64 KiB だけを見る。FILE_DESCRIPTION を長くして、FILE_SCHEMA の文字列の閉じる引用符を 64 KiB の最後の
/// バイトに置くと読み、その次のバイトに置くと FILE_SCHEMA のないファイルになる
#[test]
fn reads_file_schema_in_the_first_64_kib() {
    const HEAD: usize = 64 * 1024;
    let quote_at = |at: usize| {
        let (before, after) = ("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('", "'),'2;1');\nFILE_SCHEMA(('IFC4");
        let padding = "x".repeat(at - before.len() - after.len());
        let bytes = format!("{before}{padding}{after}'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n").into_bytes();
        assert_eq!(bytes[at], b'\'');
        bytes
    };
    assert_eq!(detect_sxf(&quote_at(HEAD - 1)), None);
    assert_eq!(detect_sxf(&quote_at(HEAD)), Some("p21"));
}

/// HEADER のメタデータやコメントにある FILE_SCHEMA の文字は宣言ではない（ISO 10303-21, 6.2・8.2.3）。
#[test]
fn schema_names_in_comments_and_metadata_are_not_declarations() {
    let bytes = step(b"/* FILE_SCHEMA(('IFC4')); */\nFILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING')); ");
    assert_eq!(detect_sxf(&bytes), Some("p21"));
    let bytes = replace(&step(SXF_SCHEMA.as_bytes()), "FILE_NAME('x'", "FILE_NAME('FILE_SCHEMA((''IFC4''))'");
    assert_eq!(detect_sxf(&bytes), Some("p21"));
    let bytes = step(b"/* FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING')); */\nFILE_SCHEMA(('IFC4')); ");
    assert_eq!(detect_sxf(&bytes), None);
    assert_eq!(detect_sxf(&step(b"FILE_SCHEMA/* metadata */( /* metadata */ ('IFC4'));")), None);
    assert_eq!(detect_sxf(&step(b"FILE_SCHEMA/* metadata */(('ASSOCIATIVE_DRAUGHTING'));")), Some("p21"));
}
