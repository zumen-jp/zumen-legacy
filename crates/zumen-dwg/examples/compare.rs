//! 同じ図面の DWG と DXF を AutoCAD の図面のモデルにして比べる（開発用）。
//!
//!   cargo run --release -p zumen-dwg --example compare -- drawing.dwg drawing.dxf [--all]
//!
//! 表・ブロック・レイアウトと、ハンドルごとの図形の値を比べる。数は相対 1e-9 まで同じとみなす。
use std::collections::BTreeMap;
use zumen_dxf::{DxfDocument, DxfEntity, parse_dxf};

fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        let numeric = c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == '+';
        if numeric {
            current.push(c);
        } else {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            out.push(c.to_string());
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn same(a: &str, b: &str) -> bool {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.len() != tb.len() {
        return false;
    }
    ta.iter().zip(&tb).all(|(x, y)| {
        if x == y {
            return true;
        }
        match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(p), Ok(q)) => (p - q).abs() <= 1e-9 * p.abs().max(q.abs()).max(1.0),
            _ => false,
        }
    })
}

/// 最初に違う所の前後
fn first_difference(a: &str, b: &str) -> String {
    let (ta, tb) = (tokens(a), tokens(b));
    let mut index = 0;
    while index < ta.len() && index < tb.len() && same(&ta[index], &tb[index]) {
        index += 1;
    }
    let from = index.saturating_sub(24);
    let show = |t: &[String]| t[from.min(t.len())..(index + 16).min(t.len())].concat();
    format!("…{}\n    vs …{}", show(&ta), show(&tb))
}

fn flatten(doc: &DxfDocument) -> BTreeMap<String, &DxfEntity> {
    let mut map = BTreeMap::new();
    for entity in &doc.entities {
        map.insert(entity.base.handle.clone(), entity);
        if let zumen_dxf::DxfEntityData::Insert(insert) = &entity.data {
            for attrib in &insert.attribs {
                map.insert(attrib.base.handle.clone(), attrib);
            }
        }
    }
    map
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all = args.iter().any(|arg| arg == "--all");
    let dwg = zumen_dwg::read_dwg(&std::fs::read(&args[0]).unwrap()).unwrap();
    let dxf = parse_dxf(&std::fs::read(&args[1]).unwrap());
    println!("DWG {} {} / DXF {} {}", dwg.version, dwg.encoding, dxf.version, dxf.encoding);
    println!("notices DWG {:?}", dwg.notices);
    println!("notices DXF {:?}", dxf.notices);
    println!("skipped DWG {:?} / DXF {:?}", dwg.skipped, dxf.skipped);
    let h = (&dwg.header, &dxf.header);
    println!(
        "header insunits {}/{} measurement {:?}/{:?} ltscale {}/{} psltscale {}/{} tilemode {}/{} dimscale {}/{} dimasz {}/{} dimtxt {}/{} textsize {:?}/{:?}",
        h.0.insunits,
        h.1.insunits,
        h.0.measurement,
        h.1.measurement,
        h.0.ltscale,
        h.1.ltscale,
        h.0.psltscale,
        h.1.psltscale,
        h.0.tilemode,
        h.1.tilemode,
        h.0.dimscale,
        h.1.dimscale,
        h.0.dimasz,
        h.1.dimasz,
        h.0.dimtxt,
        h.1.dimtxt,
        h.0.textsize,
        h.1.textsize
    );
    let list = |name: &str, a: Vec<String>, b: Vec<String>| {
        if a == b {
            println!("{name}: same ({})", a.len());
        } else {
            println!("{name}: DWG {a:?}\n{name}: DXF {b:?}");
        }
    };
    list(
        "layers",
        dwg.layers.iter().map(|l| format!("{l:?}")).collect(),
        dxf.layers.iter().map(|l| format!("{l:?}")).collect(),
    );
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    list(
        "linetypes",
        sorted(dwg.linetypes.iter().map(|l| format!("{l:?}")).collect()),
        sorted(dxf.linetypes.iter().map(|l| format!("{l:?}")).collect()),
    );
    list(
        "styles",
        sorted(dwg.styles.iter().map(|l| format!("{l:?}")).collect()),
        sorted(dxf.styles.iter().map(|l| format!("{l:?}")).collect()),
    );
    list(
        "dimstyles",
        sorted(dwg.dimstyles.iter().map(|l| format!("{l:?}")).collect()),
        sorted(dxf.dimstyles.iter().map(|l| format!("{l:?}")).collect()),
    );
    list(
        "mline_styles",
        sorted(dwg.mline_styles.iter().flatten().map(|l| format!("{l:?}")).collect()),
        sorted(dxf.mline_styles.iter().flatten().map(|l| format!("{l:?}")).collect()),
    );
    list(
        "blocks",
        sorted(dwg.blocks.iter().map(|l| format!("{l:?}")).collect()),
        sorted(dxf.blocks.iter().map(|l| format!("{l:?}")).collect()),
    );
    list(
        "layouts",
        dwg.layouts.iter().map(|l| format!("{l:?}")).collect(),
        dxf.layouts.iter().map(|l| format!("{l:?}")).collect(),
    );
    let keys = |doc: &DxfDocument| doc.sort_handles.iter().map(|(k, v)| format!("{k} {}", v.len())).collect::<Vec<_>>();
    list("sort_handles", keys(&dwg), keys(&dxf));
    let (a, b) = (flatten(&dwg), flatten(&dxf));
    let mut missing: BTreeMap<String, u32> = BTreeMap::new();
    let mut extra: BTreeMap<String, u32> = BTreeMap::new();
    let mut differ: BTreeMap<String, u32> = BTreeMap::new();
    let mut same_count = 0;
    let mut shown = 0;
    for (handle, entity) in &b {
        match a.get(handle) {
            None => *missing.entry(entity.base.kind.clone()).or_default() += 1,
            Some(other) => {
                let mut x = (*other).clone();
                let mut y = (*entity).clone();
                if let zumen_dxf::DxfEntityData::Insert(i) = &mut x.data {
                    i.attribs.clear();
                }
                if let zumen_dxf::DxfEntityData::Insert(i) = &mut y.data {
                    i.attribs.clear();
                }
                // DXF は Standard の文字スタイル（7）を省くので、名前は大文字と小文字を区別しない
                let (da, db) = (format!("{:?}", x).replace("\"Standard\"", "\"STANDARD\""), format!("{:?}", y));
                if std::env::var("SHOW").is_ok_and(|show| show == *handle) {
                    println!("SHOW {handle}\n  DWG {da}\n  DXF {db}");
                }
                if same(&da, &db) {
                    same_count += 1;
                } else {
                    *differ.entry(entity.base.kind.clone()).or_default() += 1;
                    if all || shown < 12 {
                        shown += 1;
                        println!("DIFF {handle} {}\n  {}", entity.base.kind, first_difference(&da, &db));
                        if std::env::var("SHOW").is_ok_and(|show| show == *handle) {
                            println!("  DWG {da}\n  DXF {db}");
                        }
                    }
                }
            }
        }
    }
    for (handle, entity) in &a {
        if !b.contains_key(handle) {
            *extra.entry(entity.base.kind.clone()).or_default() += 1;
        }
    }
    println!("entities: same {same_count}, differ {differ:?}, missing in DWG {missing:?}, extra in DWG {extra:?}");
    // 表示用の図面を、すべての表示で比べる（形式・版・メタデータは比べない）
    let mut views = vec![None];
    views.extend(dxf.layouts.iter().filter(|layout| !layout.model).map(|layout| Some(layout.name.clone())));
    views.push(Some("Model".to_string()));
    for view in views {
        let mut a = serde_json::to_value(zumen_dxf::dxf_to_scene(&dwg, view.as_deref())).unwrap();
        let mut b = serde_json::to_value(zumen_dxf::dxf_to_scene(&dxf, view.as_deref())).unwrap();
        for scene in [&mut a, &mut b] {
            let object = scene.as_object_mut().unwrap();
            object.remove("format");
            object.remove("version");
            object.remove("metadata");
        }
        let mut out = Vec::new();
        json_differences(&a, &b, "", &mut out);
        let count = a["entities"].as_array().map_or(0, Vec::len);
        if out.is_empty() {
            println!("scene {:?}: same ({count} entities)", view);
        } else {
            println!("scene {:?}: {} differences ({count} entities)\n  {}", view, out.len(), out.join("\n  "));
        }
    }
}

/// 数を相対 1e-9 まで同じとみなして 2 つの JSON を比べ、違う所を返す
pub fn json_differences(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    use serde_json::Value;
    if out.len() > 40 {
        return;
    }
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (p, q) = (x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN));
            let close = (p - q).abs() <= 1e-9 * p.abs().max(q.abs()).max(1.0);
            if !close {
                out.push(format!("{path}: {p} / {q}"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: {} items / {}", x.len(), y.len()));
            }
            for (index, (p, q)) in x.iter().zip(y).enumerate() {
                json_differences(p, q, &format!("{path}[{index}]"), out);
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (key, p) in x {
                match y.get(key) {
                    Some(q) => json_differences(p, q, &format!("{path}.{key}"), out),
                    None => out.push(format!("{path}.{key}: only in DWG")),
                }
            }
            for key in y.keys() {
                if !x.contains_key(key) {
                    out.push(format!("{path}.{key}: only in DXF"));
                }
            }
        }
        _ => {
            if a != b {
                out.push(format!(
                    "{path}: {} / {}",
                    a.to_string().chars().take(160).collect::<String>(),
                    b.to_string().chars().take(160).collect::<String>()
                ));
            }
        }
    }
}
