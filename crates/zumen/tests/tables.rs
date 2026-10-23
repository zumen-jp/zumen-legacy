//! 罫線で描いた表（zumen_core::tables）: 作図キットの見本を各形式に書いて読み直し、表の行・列と文字を確かめる。

use zumen::tables::{RuledTable, TableOptions, ruled_tables};
use zumen::{DraftOptions, ReadDrawingOptions, WriteOptions};

const SPEC: &str = include_str!("../../zumen-draft/examples/sheets.json");

fn tables(to: &str, sheet: &str) -> Vec<RuledTable> {
    let options = DraftOptions { write: WriteOptions::default(), sheet: Some(sheet.to_string()) };
    let drafted = zumen::draft(SPEC, to, &options).unwrap();
    let scene = zumen::read(&drafted.bytes, &ReadDrawingOptions::default()).unwrap();
    ruled_tables(&scene, &TableOptions::default())
}

fn texts(table: &RuledTable) -> Vec<Vec<String>> {
    table.grid()
}

#[test]
fn finds_the_schedules_and_the_title_block_of_a_drafted_sheet_in_every_format() {
    for to in ["dxf", "dwg", "jww", "sfc"] {
        let found = tables(to, "A01-1");
        let sizes: Vec<(usize, usize)> = found.iter().map(RuledTable::size).collect();
        // 表題欄（図枠の右の 1 列）、工事概要と面積の表、外部仕上表
        assert_eq!(sizes, [(6, 1), (13, 5), (7, 3)], "{to}");

        let title = texts(&found[0]);
        assert_eq!(title[5], ["図面番号\nA01-1"], "{to}");
        assert_eq!(found[0].bbox.min_x, 370.0, "{to}");

        let summary = &found[1];
        let grid = texts(summary);
        assert_eq!(grid[1][..2], ["工事名称", "(仮称)サンプル計画 新築工事"], "{to}");
        assert_eq!(grid[8], ["建築面積", "", "616.12 ㎡", "0.00 ㎡", "616.12 ㎡"], "{to}");
        // 見出しは 5 列を結合したます目
        let heading = &summary.cells[0];
        assert_eq!((heading.text.as_str(), heading.column_span, heading.row_span), ("■工事概要", 5, 1), "{to}");
        assert_eq!(summary.columns, [25.0, 55.0, 115.0, 145.0, 175.0, 205.0], "{to}");

        let finish = texts(&found[2]);
        assert_eq!(finish[1], ["記号", "部位", "材料・規格・寸法"], "{to}");
        // 折り返した文字は行ごとに改行でつなぐ
        assert_eq!(finish[3][2], "デッキコンクリート合成スラブ t80\n改質アスファルトシート防水 断熱材 t50", "{to}");
    }
}

#[test]
fn finds_the_title_block_beside_a_plan_without_taking_its_rooms() {
    for to in ["dxf", "jww", "sfc"] {
        let found = tables(to, "A10-1");
        assert_eq!(found.len(), 1, "{to}");
        let grid = texts(&found[0]);
        assert_eq!(grid[1], ["図面名称", "1階平面図"], "{to}");
        assert_eq!(grid[3], ["図面番号", "A10-1"], "{to}");
    }
}
