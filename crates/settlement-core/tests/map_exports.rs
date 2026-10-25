//! Exports and imports: SVG, PNG and structured map data; one district or
//! all districts (never a joined city); settlement exports that carry every
//! map record and import as a new settlement; refusing bad files; Unicode
//! names; damaged map data that can be discarded; and stored geometry that
//! is never redrawn by an engine update.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use settlement_core::archive::SettlementArchive;
use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::ResultId;
use settlement_core::maps::export::ExportFormat;
use settlement_core::maps::render::RenderOptions;
use settlement_core::maps::{
    AssignOptions, BuildingEdits, DamagedKind, MapGenerationOptions, MapStatus, PlaceOn, SavedView,
    SeedChoice, UserValue,
};
use settlement_core::service::{AppService, MapDataExport, MapExportRequest, SettlementMapsView};
use settlement_core::settlements::SettlementId;

const UNICODE: &str = "Ærøskøbing – Ōshū 城 <&>";

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "settlement-exports-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    fn db(&self) -> PathBuf {
        self.0.join("data.sqlite3")
    }

    fn files(&self, sub: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.0.join(sub))
            .map(|d| {
                d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn saved(s: &mut AppService, size: &str, seed: u64, name: &str) -> SettlementId {
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(seed))
        .unwrap();
    let mut sizes = Selections::new();
    sizes.insert("table.size".into(), size.into());
    active
        .session
        .run_automatic(&sizes, &mut RandomFaces::seeded(seed + 1))
        .unwrap();
    s.save_new(&active, name).unwrap().id
}

fn draw(s: &mut AppService, id: &SettlementId) -> SettlementMapsView {
    s.maps_generate(
        id,
        &MapGenerationOptions {
            seed: SeedChoice::Fixed(5),
            ..Default::default()
        },
    )
    .unwrap()
}

fn request(dir: &Path, formats: &[ExportFormat]) -> MapExportRequest {
    MapExportRequest {
        map: None,
        formats: formats.to_vec(),
        options: RenderOptions {
            width_px: 600,
            ..Default::default()
        },
        dir: Some(dir.to_path_buf()),
    }
}

/// A mapped Capital with a renamed district, building details, a placed
/// location and a point of interest on two districts, and a saved view.
fn annotated_capital(s: &mut AppService) -> SettlementId {
    for seed in 1.. {
        let id = saved(s, "capital", seed, UNICODE);
        let view = draw(s, &id);
        let set = view.maps.as_ref().unwrap();
        if set.maps.len() < 2 {
            continue;
        }
        let (a, b) = (&set.maps[0], &set.maps[1]);
        let Some(loc) = view
            .locations
            .iter()
            .find(|l| l.district.as_ref() == a.district())
        else {
            continue;
        };
        let building = a.geometry.buildings[0].id.clone();
        s.maps_building(&id, &a.id, &building).unwrap();
        s.maps_building_edit(
            &id,
            &a.id,
            &building,
            1,
            BuildingEdits {
                name: Some(UserValue::new(Some("Ōshū Hall".into()))),
                notes: "多言語のメモ".into(),
                ..Default::default()
            },
        )
        .unwrap();
        s.maps_location_assign(
            &id,
            &loc.id.clone(),
            &PlaceOn {
                map_id: a.id.clone(),
                building_id: building,
            },
            AssignOptions::default(),
        )
        .unwrap();
        s.maps_poi_add(
            &id,
            &b.id,
            b.geometry.buildings[1].centroid,
            "Brunnen".into(),
            String::new(),
        )
        .unwrap();
        s.maps_district_rename(&id, &b.district().unwrap().id.clone(), Some("Hafen".into()))
            .unwrap();
        s.maps_save_view(
            &id,
            &SavedView {
                map_id: a.id.clone(),
                centre: a.geometry.buildings[0].centroid,
                zoom: 2.5,
            },
        )
        .unwrap();
        return id;
    }
    unreachable!()
}

#[test]
fn a_town_exports_as_svg_png_and_data() {
    let tmp = Temp::new();
    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let id = saved(&mut s, "town", 3, UNICODE);
    let view = draw(&mut s, &id);
    let loc = view.locations[0].id.clone();
    let map = &view.maps.as_ref().unwrap().maps[0];
    s.maps_location_assign(
        &id,
        &loc,
        &PlaceOn {
            map_id: map.id.clone(),
            building_id: map.geometry.buildings[0].id.clone(),
        },
        AssignOptions::default(),
    )
    .unwrap();
    let out = tmp.0.join("out");
    let result = s
        .maps_export(
            &id,
            &request(
                &out,
                &[ExportFormat::Svg, ExportFormat::Png, ExportFormat::Json],
            ),
        )
        .unwrap();
    let stem = "Ærøskøbing – Ōshū 城 _&_";
    assert_eq!(
        tmp.files("out"),
        [
            format!("{stem}.map.json"),
            format!("{stem}.png"),
            format!("{stem}.svg")
        ]
    );
    assert_eq!(result.files.len(), 3);

    let svg = std::fs::read_to_string(out.join(format!("{stem}.svg"))).unwrap();
    assert!(svg.starts_with("<?xml"));
    assert!(
        svg.contains("<title>Ærøskøbing – Ōshū 城 &lt;&amp;&gt;</title>"),
        "escaped"
    );
    assert!(svg.contains("Numbered locations"), "with the legend");
    let png = std::fs::read(out.join(format!("{stem}.png"))).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert!(png.len() > 2000);

    let data: MapDataExport = serde_json::from_str(
        &std::fs::read_to_string(out.join(format!("{stem}.map.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(data.format, "settlement-generator.map-data");
    assert_eq!(data.settlement_name, UNICODE);
    assert_eq!(data.maps.len(), 1);
    assert_eq!(data.maps[0].geometry, map.geometry);
    assert_eq!(data.legends[0].entries[0].number, 1);

    // Without markers and legend: neither is drawn.
    let mut plain = request(&tmp.0.join("plain"), &[ExportFormat::Svg]);
    plain.options.markers = false;
    plain.options.legend = false;
    s.maps_export(&id, &plain).unwrap();
    let svg = std::fs::read_to_string(tmp.0.join("plain").join(format!("{stem}.svg"))).unwrap();
    assert!(!svg.contains("Numbered locations"));
}

#[test]
fn districts_export_one_at_a_time_or_all_but_never_joined() {
    let tmp = Temp::new();
    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let id = annotated_capital(&mut s);
    let view = s.maps_get(&id).unwrap();
    let set = view.maps.as_ref().unwrap();

    // All existing district maps: one file each, named by district.
    s.maps_export(&id, &request(&tmp.0.join("all"), &[ExportFormat::Svg]))
        .unwrap();
    let files = tmp.files("all");
    assert_eq!(files.len(), set.maps.len());
    for map in &set.maps {
        assert!(
            files
                .iter()
                .any(|f| f.ends_with(&format!(" - {}.svg", map.label))),
            "{files:?}"
        );
    }
    assert!(
        files.iter().any(|f| f.contains("Hafen")),
        "the user's district name"
    );

    // One district, with its own data file.
    let one = set.maps[1].id.clone();
    let mut single = request(&tmp.0.join("one"), &[ExportFormat::Svg, ExportFormat::Json]);
    single.map = Some(one.clone());
    s.maps_export(&id, &single).unwrap();
    assert_eq!(tmp.files("one").len(), 2);
    let json = tmp
        .files("one")
        .into_iter()
        .find(|f| f.ends_with(".map.json"))
        .unwrap();
    let data: MapDataExport =
        serde_json::from_str(&std::fs::read_to_string(tmp.0.join("one").join(json)).unwrap())
            .unwrap();
    assert_eq!(data.maps.len(), 1);
    assert_eq!(data.maps[0].id, one);
    assert_eq!(data.pois.len(), 1);
    assert!(data.buildings.iter().all(|b| b.map_id == one));

    // A district that is not drawn cannot be exported.
    let mut missing = single.clone();
    missing.map = Some(settlement_core::ids::MapId::from("map-district-99"));
    assert!(s.maps_export(&id, &missing).is_err());
}

#[test]
fn a_settlement_export_imports_as_a_new_settlement_with_every_map_record() {
    let tmp = Temp::new();
    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let id = annotated_capital(&mut s);
    let before = s.maps_get(&id).unwrap();
    let file = s
        .settlement_export(&id, Some(&tmp.0.join("backup")))
        .unwrap();
    assert!(file.to_string_lossy().ends_with(".settlement.json"));

    // Into another database, as after a reinstall.
    let mut other = AppService::new(Database::open(&tmp.0.join("other.sqlite3")).unwrap());
    let imported = other.settlement_import_file(&file).unwrap();
    assert_eq!(imported.name, UNICODE);
    assert_ne!(imported.id, id, "always a new settlement");
    let after = other.maps_get(&imported.id).unwrap();
    assert_eq!(
        other.saved_get(&imported.id).unwrap().stages,
        s.saved_get(&id).unwrap().stages
    );
    let geometry = |v: &SettlementMapsView| -> Vec<_> {
        v.maps
            .as_ref()
            .unwrap()
            .maps
            .iter()
            .map(|m| {
                (
                    m.id.clone(),
                    m.label.clone(),
                    m.seed,
                    m.engine.clone(),
                    m.geometry
                        .buildings
                        .iter()
                        .map(|b| (b.id.clone(), b.footprint.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    };
    assert_eq!(geometry(&after), geometry(&before));
    assert_eq!(after.planned, before.planned);
    assert_eq!(after.locations, before.locations);
    assert_eq!(after.pois, before.pois);
    assert_eq!(after.legends, before.legends);
    assert_eq!(after.views, before.views);
    assert_eq!(after.buildings, before.buildings);
    assert_eq!(after.status, MapStatus::Current);
    for m in &after.maps.as_ref().unwrap().maps {
        assert_eq!(m.settlement_id, imported.id);
        assert!(
            m.geometry
                .buildings
                .iter()
                .all(|b| b.settlement_id == imported.id)
        );
    }
    let b = &before.buildings[0];
    let original = s.maps_building(&id, &b.map_id, &b.building_id).unwrap();
    let copy = other
        .maps_building(&imported.id, &b.map_id, &b.building_id)
        .unwrap();
    assert_eq!(copy.name, original.name);
    assert_eq!(copy.notes, "多言語のメモ");
    assert_eq!(copy.version, original.version);
    // The copy goes on numbering where the original left off.
    let map = &after.maps.as_ref().unwrap().maps[0];
    let more = other
        .maps_poi_add(
            &imported.id,
            &map.id,
            map.geometry.buildings[2].centroid,
            "Neu".into(),
            String::new(),
        )
        .unwrap();
    assert_eq!(more.pois.iter().map(|p| p.number).max(), Some(3));
    // Importing twice gives two settlements; the original is untouched.
    other.settlement_import_file(&file).unwrap();
    assert_eq!(other.saved_list().unwrap().len(), 2);
    assert_eq!(s.maps_get(&id).unwrap(), before);
}

#[test]
fn bad_imports_are_refused_and_store_nothing() {
    let tmp = Temp::new();
    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let id = annotated_capital(&mut s);
    let good = s.settlement_archive(&id).unwrap();
    let count = s.saved_list().unwrap().len();

    let mut cases: Vec<(String, &str)> = vec![
        ("not json".into(), "not valid JSON"),
        (
            r#"{"format":"something-else"}"#.into(),
            "not a Settlement Generator",
        ),
    ];
    let mut newer = serde_json::to_value(&good).unwrap();
    newer["format_version"] = 99.into();
    cases.push((newer.to_string(), "newer version"));
    let mut newer_maps = good.clone();
    newer_maps.maps.as_mut().unwrap().map_format = 99;
    cases.push((newer_maps.to_json(), "newer version"));
    let mut doubled = good.clone();
    let maps = doubled.maps.as_mut().unwrap();
    maps.buildings.push(maps.buildings[0].clone());
    cases.push((doubled.to_json(), "appears twice"));
    let mut numbering = good.clone();
    numbering.maps.as_mut().unwrap().next_marker = 1;
    cases.push((numbering.to_json(), "marker number"));
    let mut wrong = serde_json::to_value(&good).unwrap();
    wrong["settlement"]["snapshot"] = "nonsense".into();
    cases.push((wrong.to_string(), "damaged"));
    let mut blank = good.clone();
    blank.settlement.name = "  ".into();
    cases.push((blank.to_json(), "empty"));

    for (text, expected) in cases {
        let error = s.settlement_import_json(&text).unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
        assert_eq!(s.saved_list().unwrap().len(), count, "nothing stored");
    }
    // The good one still imports.
    let text = good.to_json();
    assert!(SettlementArchive::from_json(&text).is_ok());
    s.settlement_import_json(&text).unwrap();
}

#[test]
fn damaged_map_data_is_reported_and_can_be_discarded() {
    let tmp = Temp::new();
    let id = {
        let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
        annotated_capital(&mut s)
    };
    let raw = rusqlite::Connection::open(tmp.db()).unwrap();
    raw.execute(
        "UPDATE settlement_map_buildings SET document = '{broken' WHERE settlement_id = ?1",
        [id.as_str()],
    )
    .unwrap();
    raw.execute(
        "UPDATE settlement_map_pois SET document = '[]' WHERE settlement_id = ?1",
        [id.as_str()],
    )
    .unwrap();
    drop(raw);

    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let view = s.maps_get(&id).unwrap();
    let kinds: Vec<DamagedKind> = view.damaged.iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [DamagedKind::Building, DamagedKind::Poi]);
    assert!(view.maps.is_some(), "the maps still open");
    assert!(!view.locations.is_empty(), "and the directory");
    assert!(
        s.settlement_archive(&id).is_err(),
        "an export would be incomplete"
    );
    let repaired = s.maps_discard_damaged(&id).unwrap();
    assert!(repaired.damaged.is_empty());
    assert!(repaired.pois.is_empty() && repaired.buildings.is_empty());
    assert!(s.settlement_archive(&id).is_ok());

    // A damaged map set: the settlement and directory still open.
    let raw = rusqlite::Connection::open(tmp.db()).unwrap();
    raw.execute(
        "UPDATE settlement_maps SET document = 'x' WHERE settlement_id = ?1",
        [id.as_str()],
    )
    .unwrap();
    drop(raw);
    let s2 = AppService::new(Database::open(&tmp.db()).unwrap());
    let view = s2.maps_get(&id).unwrap();
    assert_eq!(view.damaged[0].kind, DamagedKind::Maps);
    assert!(view.maps.is_none());
    assert!(s2.saved_get(&id).is_ok());
    drop(s2);
    let mut s3 = AppService::new(Database::open(&tmp.db()).unwrap());
    let cleared = s3.maps_discard_damaged(&id).unwrap();
    assert!(cleared.damaged.is_empty());
    assert_eq!(cleared.status, MapStatus::NotGenerated);
    draw(&mut s3, &id);
}

#[test]
fn an_engine_update_never_redraws_stored_geometry() {
    let tmp = Temp::new();
    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let id = saved(&mut s, "village", 2, "Old Mill");
    draw(&mut s, &id);
    // A map stored by an older engine version, as after an app update.
    let mut archive = s.settlement_archive(&id).unwrap();
    for m in &mut archive.maps.as_mut().unwrap().set.maps {
        m.engine.version = "0.9.0".into();
    }
    let old = s.settlement_import_json(&archive.to_json()).unwrap();
    let view = s.maps_get(&old.id).unwrap();
    let map = &view.maps.as_ref().unwrap().maps[0];
    assert_eq!(map.engine.version, "0.9.0", "read back as stored");
    // The same geometry (only the owning settlement's ID is the copy's).
    let mut stored = archive.maps.as_ref().unwrap().set.maps[0].geometry.clone();
    for b in &mut stored.buildings {
        b.settlement_id = old.id.clone();
    }
    assert_eq!(map.geometry, stored);
    assert_eq!(
        view.status,
        MapStatus::Current,
        "not reported as needing a redraw"
    );
    assert_ne!(
        view.engine.version, "0.9.0",
        "new maps would use the current engine"
    );
}
