//! 図形でないオブジェクト: 名前付きオブジェクトの辞書と標準の辞書（ACAD_COLOR・ACAD_GROUP・ACAD_LAYOUT・ACAD_MATERIAL・
//! ACAD_MLEADERSTYLE・ACAD_MLINESTYLE・ACAD_PLOTSETTINGS・ACAD_PLOTSTYLENAME・ACAD_SCALELIST・ACAD_TABLESTYLE・
//! ACAD_VISUALSTYLE・AcDbVariableDictionary）、LAYOUT、MLINESTYLE、MLEADERSTYLE、MATERIAL、SCALE、
//! DICTIONARYVAR、ACDBPLACEHOLDER、描画順（SORTENTSTABLE）と、クラスの定義（10）

use super::{Writer, href, owned, rad, upper};
use crate::codec::RawText;
use crate::codec::object::ObjectCommon;
use crate::codec::objects::{
    Dictionary, DictionaryVar, ImageDef, ImageDefReactor, Layout, MLeaderStyle, MLineElement, MLineStyle, Material,
    MaterialColor, MaterialMap, Placeholder, Scale, SortEntsTable,
};
use crate::file::Version;
use crate::write::classes::ClassDef;

/// クラスの定義（10）。値は AutoCAD が書く値
pub(crate) fn class_def(name: &str, number: u16, instances: i32) -> Option<ClassDef> {
    const ODBX: &str = "ObjectDBX Classes";
    let (proxy_flags, app_name, cpp_name, entity, dwg_version, maintenance) = match name {
        "ACDBDICTIONARYWDFLT" => (0, ODBX, "AcDbDictionaryWithDefault", false, 22, 42),
        "ACDBPLACEHOLDER" => (0, ODBX, "AcDbPlaceHolder", false, 22, 42),
        "LAYOUT" => (0, ODBX, "AcDbLayout", false, 22, 42),
        "DICTIONARYVAR" => (0, ODBX, "AcDbDictionaryVar", false, 20, 0),
        "MATERIAL" => (1153, ODBX, "AcDbMaterial", false, 23, 14),
        "SCALE" => (1153, ODBX, "AcDbScale", false, 27, 1),
        "SORTENTSTABLE" => (0, ODBX, "AcDbSortentsTable", false, 20, 0),
        "MLEADERSTYLE" => (4095, "ACDB_MLEADERSTYLE_CLASS", "AcDbMLeaderStyle", false, 33, 329),
        "MULTILEADER" => (1025, "ACDB_MLEADER_CLASS", "AcDbMLeader", true, 33, 329),
        "WIPEOUT" => (2175, "WipeOut", "AcDbWipeout", true, 33, 329),
        // 画像（ezdxf の classes.py と同じ印）
        "IMAGE" => (2175, "ISM", "AcDbRasterImage", true, 0, 0),
        "IMAGEDEF" => (0, "ISM", "AcDbRasterImageDef", false, 0, 0),
        "IMAGEDEF_REACTOR" => (1, "ISM", "AcDbRasterImageDefReactor", false, 0, 0),
        // R14 では LWPOLYLINE・HATCH はクラスの型（R2000 から決まった型 77・78）
        "LWPOLYLINE" => (0, ODBX, "AcDbPolyline", true, 0, 0),
        "HATCH" => (0, ODBX, "AcDbHatch", true, 0, 0),
        "ARC_DIMENSION" => (1025, ODBX, "AcDbArcDimension", true, 25, 0),
        "LARGE_RADIAL_DIMENSION" => (1025, ODBX, "AcDbRadialDimensionLarge", true, 25, 0),
        _ => return None,
    };
    Some(ClassDef {
        number,
        proxy_flags,
        app_name: app_name.into(),
        cpp_name: cpp_name.into(),
        dxf_name: name.into(),
        terminated: true,
        zombie: false,
        item_class: if entity { 0x1f2 } else { 0x1f3 },
        instances,
        dwg_version,
        maintenance,
        unknown: [0, 0],
    })
}

/// MLINESTYLE の DXF の 70 の印を DWG のビットに（読み取りの逆。1↔2、32↔64、512↔1024）
fn mline_flags(dxf: f64) -> i16 {
    let dxf = zumen_core::js::to_int32(dxf);
    let pairs = [(1, 2), (2, 1), (16, 16), (32, 64), (64, 32), (256, 256), (512, 1024), (1024, 512)];
    pairs.iter().filter(|(_, from)| dxf & from != 0).fold(0, |sum, (dwg, _)| sum | dwg) as i16
}

/// 標準の MATERIAL（ByBlock・ByLayer・Global）の値（AutoCAD が書く値）
fn material(common: ObjectCommon, name: RawText, empty: RawText, map_scale: f64) -> Material {
    let color = MaterialColor { method: 0, factor: 1.0, rgb: None };
    let map = |scale: f64| MaterialMap {
        blend: 1.0,
        projection: 2,
        tiling: 1,
        auto_transform: 1,
        matrix: [scale, 0.0, 0.0, 0.0, 0.0, scale, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        source: 1,
        file: Some(empty.clone()),
    };
    let mut refraction_map = map(1.0);
    refraction_map.source = 0;
    refraction_map.file = None;
    Material {
        common,
        name,
        description: empty.clone(),
        ambient: color.clone(),
        diffuse: color.clone(),
        diffuse_map: map(map_scale),
        specular: color,
        specular_map: map(1.0),
        gloss: 0.5,
        reflection_map: map(map_scale),
        opacity: 1.0,
        opacity_map: map(map_scale),
        bump_map: map(map_scale),
        refraction_index: 1.0,
        refraction_map,
        tail: Some(([0.0; 3], [0, 63, 0])),
    }
}

impl Writer<'_> {
    fn dictionary(&mut self, handle: u64, owner: u64, entries: &[(String, u64)]) {
        let dictionary = Dictionary {
            common: if owner == 0 { owned(0, false) } else { owned(owner, true) },
            cloning: 1,
            hard_owner: 0,
            r14_unknown: 0,
            entries: entries.iter().map(|(name, entry)| (self.text(name), href(2, *entry))).collect(),
            default: None,
        };
        let mut out = self.object(42, handle);
        dictionary.write(&mut out);
        self.push(&out);
    }

    /// 図形でないオブジェクトをすべて書く
    pub(super) fn write_objects(&mut self) {
        let fixed = std::mem::take(&mut self.fixed);
        let root = fixed.root;
        let images = (!self.image_defs.is_empty()).then(|| self.handles.fresh());
        let mut entries: Vec<(String, u64)> = [
            ("ACAD_COLOR", fixed.color_dict),
            ("ACAD_GROUP", fixed.group_dict),
            ("ACAD_LAYOUT", fixed.layout_dict),
            ("ACAD_MATERIAL", fixed.material_dict),
            ("ACAD_MLEADERSTYLE", fixed.mleader_dict),
            ("ACAD_MLINESTYLE", fixed.mline_dict),
            ("ACAD_PLOTSETTINGS", fixed.plot_dict),
            ("ACAD_PLOTSTYLENAME", fixed.plotstyle_dict),
            ("ACAD_SCALELIST", fixed.scale_dict),
            ("ACAD_TABLESTYLE", fixed.tablestyle_dict),
            ("ACAD_VISUALSTYLE", fixed.visualstyle_dict),
            ("AcDbVariableDictionary", fixed.variable_dict),
        ]
        .into_iter()
        .map(|(name, handle)| (name.to_string(), handle))
        .collect();
        if let Some(images) = images {
            entries.insert(2, (zumen_dxf::image::DICTIONARY.into(), images));
        }
        self.dictionary(root, 0, &entries);
        if let Some(images) = images {
            self.write_images(images, root);
        }
        for handle in
            [fixed.color_dict, fixed.group_dict, fixed.plot_dict, fixed.tablestyle_dict, fixed.visualstyle_dict]
        {
            self.dictionary(handle, root, &[]);
        }
        let layouts: Vec<(String, u64)> =
            self.layouts.iter().map(|layout| (layout.name.clone(), layout.handle)).collect();
        self.dictionary(fixed.layout_dict, root, &layouts);
        self.dictionary(
            fixed.material_dict,
            root,
            &[
                ("ByBlock".into(), fixed.material_byblock),
                ("ByLayer".into(), fixed.material_bylayer),
                ("Global".into(), fixed.material_global),
            ],
        );
        self.dictionary(fixed.mleader_dict, root, &[("Standard".into(), fixed.mleader_style)]);
        // 辞書の名前は AutoCAD と同じく Standard（スタイルの名前は STANDARD）
        let mlines: Vec<(String, u64)> = self
            .mline_styles
            .iter()
            .map(|(handle, style)| {
                let key = if upper(&style.name) == "STANDARD" { "Standard".into() } else { style.name.clone() };
                (key, *handle)
            })
            .collect();
        self.dictionary(fixed.mline_dict, root, &mlines);
        self.dictionary(fixed.scale_dict, root, &[("A0".into(), fixed.scale)]);
        let variables: Vec<(String, u64)> =
            fixed.variables.iter().map(|(name, handle, _)| (name.to_string(), *handle)).collect();
        self.dictionary(fixed.variable_dict, root, &variables);

        // ACAD_PLOTSTYLENAME（既定の Normal を持つ辞書）と、レイヤが参照する Normal
        let number = self.class_number("ACDBDICTIONARYWDFLT");
        let plotstyles = Dictionary {
            common: owned(root, true),
            cloning: 1,
            hard_owner: 0,
            r14_unknown: 0,
            entries: vec![(self.text("Normal"), href(2, fixed.placeholder))],
            default: Some(href(5, fixed.placeholder)),
        };
        let mut out = self.object(number, fixed.plotstyle_dict);
        plotstyles.write(&mut out);
        self.push(&out);
        let number = self.class_number("ACDBPLACEHOLDER");
        let mut out = self.object(number, fixed.placeholder);
        Placeholder { common: owned(fixed.plotstyle_dict, true) }.write(&mut out);
        self.push(&out);

        self.write_layouts(&fixed);
        self.write_mline_styles(&fixed);
        self.write_mleader_style(&fixed);
        for (name, handle, scale) in [
            ("ByBlock", fixed.material_byblock, 1.0),
            ("ByLayer", fixed.material_bylayer, 1.0),
            ("Global", fixed.material_global, 0.0208),
        ] {
            let number = self.class_number("MATERIAL");
            let mut out = self.object(number, handle);
            material(owned(fixed.material_dict, true), self.text(name), self.empty(), scale).write(&mut out);
            self.push(&out);
        }
        let number = self.class_number("SCALE");
        let scale = Scale {
            common: owned(fixed.scale_dict, true),
            unknown: 0,
            name: self.text("1:1"),
            paper_units: 1.0,
            drawing_units: 1.0,
            unit_scale: true,
        };
        let mut out = self.object(number, fixed.scale);
        scale.write(&mut out);
        self.push(&out);
        for (_, handle, value) in &fixed.variables {
            let number = self.class_number("DICTIONARYVAR");
            let variable =
                DictionaryVar { common: owned(fixed.variable_dict, true), schema: 0, value: self.text(value) };
            let mut out = self.object(number, *handle);
            variable.write(&mut out);
            self.push(&out);
        }
        self.write_sort_tables();
        self.fixed = fixed;
    }

    /// 画像の辞書（ACAD_IMAGE_DICT）と、画像の定義（IMAGEDEF、20.4.82）・リアクタ（IMAGEDEF_REACTOR、20.4.83）。
    /// 値は DXF の書き出しと同じ
    fn write_images(&mut self, dictionary: u64, root: u64) {
        let definitions = std::mem::take(&mut self.image_defs);
        let mut entries: Vec<(String, u64)> = Vec::new();
        for definition in &definitions {
            let name = zumen_dxf::image::entry_name(&definition.path, |name| {
                entries.iter().any(|(used, _)| used.eq_ignore_ascii_case(name))
            });
            entries.push((name, definition.handle));
        }
        self.dictionary(dictionary, root, &entries);
        for definition in &definitions {
            let number = self.class_number("IMAGEDEF");
            let mut common = owned(dictionary, true);
            common.reactors.extend(definition.reactors.iter().map(|(reactor, _)| href(4, *reactor)));
            let value = ImageDef {
                common,
                class_version: 0,
                size: definition.size,
                path: self.text(&definition.path),
                loaded: true,
                resolution_units: 0,
                pixel_size: [1.0, 1.0],
            };
            let mut out = self.object(number, definition.handle);
            value.write(&mut out);
            self.push(&out);
            for (reactor, image) in &definition.reactors {
                let number = self.class_number("IMAGEDEF_REACTOR");
                let mut out = self.object(number, *reactor);
                ImageDefReactor { common: owned(*image, false), class_version: 2 }.write(&mut out);
                self.push(&out);
            }
        }
        self.image_defs = definitions;
    }

    /// LAYOUT（20.4.84）と、その前の PLOTSETTINGS の値。値は DXF の書き出しと同じ（モデルのレイアウトがあればその値）
    fn write_layouts(&mut self, fixed: &super::Fixed) {
        for tab in 0..self.layouts.len() {
            let (name, handle, block, source) = {
                let layout = &self.layouts[tab];
                (layout.name.clone(), layout.handle, layout.block.clone(), layout.source)
            };
            let block = &self.blocks[self.block_index[&upper(&block)]];
            let record = block.record;
            let viewports: Vec<u64> =
                block.entities.iter().copied().filter(|entity| self.viewports.contains(entity)).collect();
            let last_viewport = if tab == 0 { fixed.vport } else { viewports.first().copied().unwrap_or(0) };
            // R2004 以降の LAYOUT は決まった型 82（クラスは数えるだけ）、R2000 はクラスの型（AutoCAD と同じ）
            let class = self.class_number("LAYOUT");
            let kind = if self.version < Version::R2004 { class } else { 82 };
            let scale = source.map_or([1.0, 1.0], |layout| layout.plot_scale);
            let layout = Layout {
                common: owned(fixed.layout_dict, true),
                page_setup: self.empty(),
                printer: self.text("none_device"),
                plot_flags: if tab == 0 { 1024 } else { 0 },
                margins: source.map_or([7.5, 20.0, 7.5, 20.0], |layout| layout.margins),
                paper: source.map_or([420.0, 297.0], |layout| [layout.paper_width, layout.paper_height]),
                paper_size: self.text(source.map_or("A3", |layout| layout.paper_size.as_str())),
                plot_origin: source.map_or([0.0, 0.0], |layout| layout.plot_origin),
                paper_units: source.map_or(1, |layout| layout.paper_units as i16),
                plot_rotation: source.map_or(0, |layout| layout.plot_rotation as i16),
                plot_type: 5,
                window_min: [0.0, 0.0],
                window_max: [0.0, 0.0],
                plot_view_name: None,
                real_units: if scale[0] > 0.0 { scale[0] } else { 1.0 },
                drawing_units: if scale[1] > 0.0 { scale[1] } else { 1.0 },
                style_sheet: self.empty(),
                scale_type: 16,
                scale_factor: 1.0,
                paper_image_origin: [0.0, 0.0],
                shade_mode: 0,
                shade_level: 2,
                shade_dpi: 300,
                name: self.text(&name),
                tab_order: tab as i32,
                flags: 1,
                ucs_origin: [0.0; 3],
                limmin: source.map_or([0.0, 0.0], |layout| layout.limmin),
                limmax: source.map_or([420.0, 297.0], |layout| layout.limmax),
                insbase: [0.0; 3],
                ucs_x: [1.0, 0.0, 0.0],
                ucs_y: [0.0, 1.0, 0.0],
                elevation: 0.0,
                ortho_type: 0,
                extmin: [1e20; 3],
                extmax: [-1e20; 3],
                plot_view: Some(href(5, 0)),
                visual_style: Some(href(4, 0)),
                block_record: href(4, record),
                last_viewport: href(4, last_viewport),
                base_ucs: href(5, 0),
                named_ucs: href(5, 0),
                viewports: viewports.into_iter().map(|viewport| href(4, viewport)).collect(),
            };
            let mut out = self.object(kind, handle);
            layout.write(&mut out);
            self.push(&out);
        }
    }

    /// MLINESTYLE（20.4.73）。R2018 より前の要素の線種は、線種の表の番号（32767 BYLAYER、32766 BYBLOCK）、R2018 以降は
    /// 線種のハンドル
    fn write_mline_styles(&mut self, fixed: &super::Fixed) {
        let entries: Vec<String> = self
            .linetypes
            .iter()
            .filter(|linetype| !matches!(upper(&linetype.name).as_str(), "BYLAYER" | "BYBLOCK"))
            .map(|linetype| upper(&linetype.name))
            .collect();
        for index in 0..self.mline_styles.len() {
            let (handle, style) = (self.mline_styles[index].0, self.mline_styles[index].1.clone());
            // MLINESTYLE (20.4.73) stores the element count as an RC (one byte).
            if style.elements.len() > 255 {
                self.notices.add(
                    zumen_core::NoticeLevel::Warning,
                    "mline-style-limit",
                    "DWG のマルチラインスタイルの要素数の上限（255）を超えるため、先頭の 255 要素だけを書きました",
                );
            }
            let elements = style
                .elements
                .iter()
                .map(|element| {
                    let name = upper(&element.linetype);
                    let linetype_index = match name.as_str() {
                        "" | "BYLAYER" => 32767,
                        "BYBLOCK" => 32766,
                        _ => entries.iter().position(|entry| *entry == name).map_or(32767, |index| index as i16),
                    };
                    // R2018 以降は線種のハンドル（BYLAYER・BYBLOCK も線種の表の項目）
                    let key = if name.is_empty() { "BYLAYER".to_string() } else { name };
                    let linetype = (self.version >= Version::R2018).then(|| {
                        href(
                            5,
                            self.linetype_handles
                                .get(&key)
                                .or(self.linetype_handles.get("BYLAYER"))
                                .copied()
                                .unwrap_or(0),
                        )
                    });
                    MLineElement {
                        offset: element.offset,
                        color: self.cmc(element.color, None),
                        linetype_index,
                        linetype,
                    }
                })
                .collect();
            let mline = MLineStyle {
                common: owned(fixed.mline_dict, true),
                name: self.text(&style.name),
                description: self.text(&style.description),
                flags: mline_flags(style.flags),
                fill_color: self.cmc(style.fill_color, None),
                start_angle: rad(style.start_angle),
                end_angle: rad(style.end_angle),
                elements,
            };
            let mut out = self.object(73, handle);
            mline.write(&mut out);
            self.push(&out);
        }
    }

    /// マルチ引出線のスタイル Standard（DXF の書き出しと同じ値）。マルチ引出線はすべての値を図形で上書きする
    fn write_mleader_style(&mut self, fixed: &super::Fixed) {
        let text_style = self.style_handles.get("STANDARD").copied().unwrap_or(0);
        let by_block = self.cmc(0.0, None);
        let linetype = self.linetype_handles.get("BYBLOCK").copied().unwrap_or(0);
        let style = MLeaderStyle {
            common: owned(fixed.mleader_dict, true),
            version: Some(2),
            content_type: 2,
            draw_mleader_order: 1,
            draw_leader_order: 0,
            max_points: 2,
            first_angle: 0.0,
            second_angle: 0.0,
            leader_type: 1,
            line_color: by_block.clone(),
            line_type: href(5, linetype),
            line_weight: -2,
            landing: true,
            landing_gap: 2.0,
            dogleg: true,
            landing_distance: 8.0,
            description: self.text("Standard"),
            arrow: href(5, 0),
            arrow_size: 4.0,
            text_default: self.empty(),
            text_style: href(5, text_style),
            left_attachment: 1,
            right_attachment: 1,
            text_angle_type: 1,
            text_alignment: 0,
            text_color: by_block.clone(),
            text_height: 4.0,
            text_frame: false,
            align_left: false,
            align_space: 4.0,
            block: href(5, 0),
            block_color: by_block,
            block_scale: [1.0, 1.0, 1.0],
            block_scale_enabled: true,
            block_rotation: 0.0,
            block_rotation_enabled: true,
            block_connection: 0,
            scale: 1.0,
            changed: false,
            annotative: false,
            break_size: 3.75,
            attachment_direction: 0,
            top_attachment: 9,
            bottom_attachment: 9,
            unknown298: false,
        };
        let number = self.class_number("MLEADERSTYLE");
        let mut out = self.object(number, fixed.mleader_style);
        style.write(&mut out);
        self.push(&out);
    }

    /// 描画順（SORTENTSTABLE、20.4.93）。ブロックの拡張辞書の ACAD_SORTENTS に置く
    fn write_sort_tables(&mut self) {
        let tables = std::mem::take(&mut self.sort_tables);
        for (index, dictionary, table, entries) in &tables {
            let record = self.blocks[*index].record;
            let xdictionary = Dictionary {
                common: owned(record, true),
                cloning: 1,
                hard_owner: 1,
                r14_unknown: 0,
                entries: vec![(self.text("ACAD_SORTENTS"), href(3, *table))],
                default: None,
            };
            let mut out = self.object(42, *dictionary);
            xdictionary.write(&mut out);
            self.push(&out);
            let number = self.class_number("SORTENTSTABLE");
            let sort = SortEntsTable {
                common: owned(*dictionary, true),
                owner: href(4, record),
                entries: entries.iter().map(|(sort, entity)| (href(0, *sort), href(4, *entity))).collect(),
            };
            let mut out = self.object(number, *table);
            sort.write(&mut out);
            self.push(&out);
        }
        self.sort_tables = tables;
    }
}
