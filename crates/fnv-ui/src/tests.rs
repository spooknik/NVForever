//! The tree on menu files written for these tests (no game files): loading, includes,
//! templates, paths, trait expressions, updates, the setters and injection.

use crate::tree::{MemSource, Origin, Source, Ui};
use crate::value::Value;

/// A small menu in the shape of the game's: a gauge prefab included twice, a template,
/// named traits on the menu that code would set.
fn files() -> MemSource {
    let mut m = MemSource::default()
        .with(
            "menus\\globals.xml",
            "<rect name=\"Strings\"> <_gap> 12 </_gap> <_dim> 128 </_dim> </rect>",
        )
        .with(
            "menus\\test_menu.xml",
            r#"<!-- a test menu -->
<menu name="TestMenu">
  <class> &TestMenu; </class>
  <_Selected> &false; </_Selected>
  <_Fill> 0.5 </_Fill>
  <rect name="Panel">
    <width> 400 </width>
    <height> <copy src="screen()" trait="height"/> <div> 2 </div> </height>
    <justify> &center; </justify>
    <rect name="Gauge"> <include src="gauge.xml"/> </rect>
    <rect name="Gauge"> <include src="gauge.xml"/> <_Fill> 1 </_Fill> </rect>
    <text name="Title">
      <string> &-sTitle; </string>
      <x> <copy src="sibling(Gauge)" trait="width"/> <add src="globals()" trait="_gap"/> </x>
      <alpha> <copy> 255 </copy> <onlyif src="io()" trait="_Selected"/> <add> 40 </add> </alpha>
      <visible> <not src="io()" trait="_Selected"/> </visible>
    </text>
    <template name="row">
      <rect name="Row"> <height> 30 </height> <include src="label.xml"/> </rect>
    </template>
  </rect>
</menu>"#,
        )
        .with(
            "menus\\prefabs\\gauge.xml",
            r#"<_Fill> 0 </_Fill>
<width> <copy src="parent()" trait="width"/> <sub> 20 </sub> </width>
<image name="Bar">
  <width> <copy src="parent()" trait="width"/> <mul src="parent()" trait="_Fill"/> </width>
  <x>
    <copy> 0 </copy>
    <sub>
      <copy src="me()" trait="width"/> <div> 2 </div>
      <onlyif> <copy src="grandparent()" trait="justify"/> <eq> &center; </eq> </onlyif>
    </sub>
  </x>
  <_pic_0> thin.dds </_pic_0>
  <_pic_1> thick.dds </_pic_1>
  <filename> <copy src="parent()" trait="_Fill"/> <ceil> 0 </ceil> <copy src="me()" trait="_pic_"/> </filename>
</image>"#,
        )
        .with("menus\\prefabs\\label.xml", "<text name=\"Label\"> <string> row </string> </text>");
    m.strings.insert("sTitle".into(), "Gauges".into());
    m
}

fn loaded() -> Ui {
    let src = files();
    let mut ui = Ui::new();
    assert!(ui.load_globals(&src));
    let s = ui.screen();
    ui.set_trait(s, "height", Value::Num(960.0));
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    ui.update();
    ui
}

#[test]
fn builds_tiles_from_the_file_and_its_includes() {
    let ui = loaded();
    assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
    let menu = ui.menu("testmenu").unwrap();
    assert_eq!(ui.tile(menu).name, "TestMenu");
    let panel = ui.find_tile("TestMenu/Panel").unwrap();
    let kids: Vec<_> = ui.tile(panel).children.iter().map(|&c| ui.tile(c).name.clone()).collect();
    assert_eq!(kids, ["Gauge", "Gauge", "Title"]);
    assert_eq!(ui.tile(panel).templates.len(), 1);
    assert_eq!(ui.get("TestMenu/class"), Some(&Value::Sym("testmenu".into())));
    assert_eq!(ui.get_string("testmenu\\panel\\title\\string").as_deref(), Some("Gauges"));
}

#[test]
fn works_traits_out() {
    let ui = loaded();
    // screen() height 960 / 2.
    assert_eq!(ui.get_float("TestMenu/Panel/height"), Some(480.0));
    // The prefab's width: the parent's 400 - 20; the bar half of it, centred.
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/width"), Some(380.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/Bar/width"), Some(0.0));
    // The second gauge's own `_Fill` (after the include) replaces the prefab's.
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:0/Bar/width"), Some(380.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:0/Bar/x"), Some(-190.0));
    // A trait name ending in `_` after a number: `_pic_` + 1.
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge:0/Bar/filename").as_deref(), Some("thick.dds"));
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge:1/Bar/filename").as_deref(), Some("thin.dds"));
    // sibling(Gauge) 380 + globals() 12.
    assert_eq!(ui.get_float("TestMenu/Panel/Title/x"), Some(392.0));
    // `_Selected` is &false;: the copy is dropped by onlyif, the add goes on.
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(40.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Title/visible"), Some(1.0));
}

#[test]
fn paths_with_wildcards_and_indexes() {
    let ui = loaded();
    assert_eq!(ui.get_float("TestMenu/*/*:1/_Fill"), Some(1.0));
    assert_eq!(ui.get_float("TestMenu/*/Gauge/_Fill"), Some(0.0));
    assert_eq!(ui.get_float("TestMenu/*:0/*:0/x"), Some(392.0));
    assert_eq!(ui.get_float("TestMenu/Panel/*:2/_Fill"), Some(0.0));
    assert!(ui.get("TestMenu/Panel/Gauge:2/_Fill").is_none());
    assert!(ui.get("TestMenu/Panel/*:3/_Fill").is_none());
    assert!(ui.find_tile("TestMenu/Panel/*:18446744073709551615").is_none());
    assert!(ui.get("NoMenu/x").is_none());
    assert!(ui.is_loaded("TestMenu/Panel/Gauge/Bar"));
    assert!(ui.is_loaded("TestMenu/Panel/width"));
    assert!(!ui.is_loaded("TestMenu/Panel/Nothing"));
    let bar = ui.find_tile("TestMenu/Panel/Gauge:0/Bar").unwrap();
    assert_eq!(ui.path_of(bar), "TestMenu/Panel/Gauge/Bar");
}

#[test]
fn setting_a_source_works_its_readers_out_again() {
    let mut ui = loaded();
    assert!(ui.set_float("TestMenu/_Selected", 1.0));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(295.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Title/visible"), Some(0.0));
    assert!(ui.set_float("TestMenu/Panel/Gauge/_Fill", 0.25));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/Bar/width"), Some(95.0));
    assert!(ui.mod_float("TestMenu/Panel/width", 100.0));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/width"), Some(480.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/Bar/width"), Some(120.0));
}

#[test]
fn a_set_trait_holds_until_its_sources_change() {
    let mut ui = loaded();
    assert!(ui.set_float("TestMenu/Panel/Title/x", 7.0));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Title/x"), Some(7.0));
    ui.set_float("TestMenu/Panel/Gauge/width", 100.0);
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Title/x"), Some(112.0));
}

#[test]
fn an_update_works_out_only_after_a_change() {
    let src = files();
    let mut ui = loaded();
    assert!(!ui.dirty());
    // The same value again, as the engine writes the HUD every frame: nothing to do.
    assert!(ui.set_float("TestMenu/_Fill", 0.5));
    let s = ui.screen();
    ui.set_trait(s, "height", Value::Num(960.0));
    assert!(!ui.dirty());
    assert_eq!(ui.update(), 0);
    assert!(ui.set_float("TestMenu/_Selected", 1.0));
    assert!(ui.dirty());
    assert!(ui.update() > 0);
    assert!(!ui.dirty());
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(295.0));
    // New tiles are worked out at the next update.
    assert!(ui.inject_text(&src, "TestMenu/Panel", r#"<text name="Added"> <x> <copy src="parent()" trait="width"/> </x> </text>"#));
    assert!(ui.dirty());
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Added/x"), Some(400.0));
    assert!(!ui.dirty());
}

#[test]
fn setters_make_no_trait() {
    let mut ui = loaded();
    assert!(!ui.set_float("TestMenu/Panel/_New", 1.0));
    assert!(!ui.set_string("TestMenu/Nope/string", "x"));
    assert!(ui.set_string("TestMenu/Panel/Title/string", "Changed"));
    assert_eq!(ui.get_string("TestMenu/Panel/Title/string").as_deref(), Some("Changed"));
}

#[test]
fn injection_adds_tiles_that_work_at_once() {
    let src = files();
    let mut ui = loaded();
    let ok = ui.inject_text(
        &src,
        "TestMenu/Panel",
        r#"<_Var1> 5 </_Var1> <text name="Added"> <x> <copy src="parent()" trait="_Var1"/> <mul> 3 </mul> </x> </text>"#,
    );
    assert!(ok);
    assert!(!ui.inject_text(&src, "TestMenu/Missing", "<rect name=\"x\"/>"));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Added/x"), Some(15.0));
    let src = files().with("menus\\prefabs\\mod\\extra.xml", "<rect name=\"Extra\"> <width> 9 </width> </rect>");
    assert!(ui.inject_file(&src, "TestMenu", "data\\menus\\prefabs\\mod\\extra.xml"));
    assert_eq!(ui.get_float("TestMenu/Extra/width"), Some(9.0));
    assert!(!ui.inject_file(&src, "TestMenu", "data\\menus\\prefabs\\none.xml"));
}

#[test]
fn templates_make_tiles() {
    let src = files();
    let mut ui = loaded();
    let made = ui.add_from_template(&src, "TestMenu/Panel/Gauge", "row", Some("Row0")).unwrap();
    assert_eq!(ui.path_of(made), "TestMenu/Panel/Gauge/Row0");
    assert_eq!(ui.get_string("TestMenu/Panel/Gauge/Row0/Label/string").as_deref(), Some("row"));
    assert!(ui.add_from_template(&src, "TestMenu/Panel", "nothing", None).is_none());
}

#[test]
fn reloading_a_menu_replaces_it_and_unloading_removes_it() {
    let src = files();
    let mut ui = loaded();
    ui.set_float("TestMenu/_Fill", 9.0);
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    ui.update();
    assert_eq!(ui.menus().count(), 1);
    assert_eq!(ui.get_float("TestMenu/_Fill"), Some(0.5));
    let gauge = ui.find_tile("TestMenu/Panel/Gauge").unwrap();
    ui.unload_tile(gauge);
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge/_Fill"), Some(1.0));
    let menu = ui.menu("TestMenu").unwrap();
    ui.unload_tile(menu);
    assert!(ui.menu("TestMenu").is_none());
    assert!(!ui.is_loaded("TestMenu"));
}

#[test]
fn loops_of_traits_stop() {
    let src = MemSource::default().with(
        "menus\\loop.xml",
        r#"<menu name="Loop"> <a> <copy src="me()" trait="b"/> <add> 1 </add> </a> <b> <copy src="me()" trait="a"/> </b> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\loop.xml").unwrap();
    ui.update();
    assert!(ui.warnings.iter().any(|w| w.contains("still changing")), "{:?}", ui.warnings);
    // Still changing: the next update works it out again.
    assert!(ui.dirty());
}

/// `src="me"` with the brackets left off is the tile itself, unless a tile bears the name
/// [observed: MCM 1.5's Options.xml places its option rows by `<copy src="me" trait="x"/>`].
#[test]
fn a_bare_me_is_the_tile_itself() {
    let src = MemSource::default()
        .with("menus\\a.xml", r#"<menu name="A"> <rect name="Panel"> <x> 40 </x> <_x1> <copy src="me" trait="x"/> <add> 20 </add> </_x1> <_x2> <copy src="Me" trait="x"/> </_x2> </rect> </menu>"#)
        .with("menus\\b.xml", r#"<menu name="B"> <rect name="me"> <x> 7 </x> </rect> <rect name="Panel"> <x> 40 </x> <_x1> <copy src="me" trait="x"/> </_x1> </rect> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\a.xml").unwrap();
    ui.load_menu(&src, "menus\\b.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("A/Panel/_x1"), Some(60.0));
    assert_eq!(ui.get_float("A/Panel/_x2"), Some(40.0));
    // A tile of that name is the one meant.
    assert_eq!(ui.get_float("B/Panel/_x1"), Some(7.0));
}

/// `src="parent"` and the other sources without an argument, brackets left off, are what
/// they name unless a tile bears the name [observed: Vanilla UI Plus's MCM OptionRect.xml,
/// `<y> <copy src="parent" trait="_y"/> </y>` on each option's title; issue #2298].
#[test]
fn a_bare_parent_is_the_parent() {
    let src = MemSource::default().with(
        "menus.xml",
        r#"<menu name="A"> <rect name="Row"> <_y> 75 </_y> <text name="title"> <y> <copy src="parent" trait="_y"/> </y>
            <_g> <copy src="Grandparent" trait="_n"/> </_g> <_s> <copy src="screen" trait="width"/> </_s> </text>
            <text name="value"> <_b> <copy src="sibling" trait="y"/> </_b> </text> <_c> <copy src="child" trait="y"/> </_c> </rect> <_n> 3 </_n> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus.xml").unwrap();
    ui.set_trait(ui.screen(), "width", Value::Num(1280.0));
    ui.update();
    assert_eq!(ui.get_float("A/Row/title/y"), Some(75.0));
    assert_eq!(ui.get_float("A/Row/title/_g"), Some(3.0));
    assert_eq!(ui.get_float("A/Row/title/_s"), Some(1280.0));
    assert_eq!(ui.get_float("A/Row/value/_b"), Some(75.0), "the tile before");
    assert_eq!(ui.get_float("A/Row/_c"), Some(75.0), "the first child");
}
#[test]
fn missing_files_are_reported_not_fatal() {
    let src = MemSource::default().with("menus\\m.xml", "<menu name=\"M\"><include src=\"gone.xml\"/><x> 1 </x></menu>");
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\m.xml").unwrap();
    assert_eq!(ui.get_float("M/x"), Some(1.0));
    assert!(ui.warnings.iter().any(|w| w.contains("gone.xml")));
    assert!(ui.load_menu(&src, "menus\\none.xml").is_err());
    let src = MemSource::default().with("menus\\p.xml", "<rect name=\"NotAMenu\"/>");
    assert!(ui.load_menu(&src, "menus\\p.xml").is_err());
}

#[test]
fn file_origins_keep_menu_and_include_paths() {
    let src = files();
    let mut ui = Ui::new();
    let path = "Menus/Test_Menu.XML";
    let root = ui.load_menu(&src, path).unwrap();
    assert_eq!(ui.tile(root).origin, Origin::File { path: path.into(), loose: false });
    let panel = ui.find_tile("TestMenu/Panel").unwrap();
    assert_eq!(ui.tile(panel).origin, ui.tile(root).origin);
    let bar = ui.find_tile("TestMenu/Panel/Gauge/Bar").unwrap();
    assert_eq!(ui.tile(bar).origin, Origin::File { path: "menus\\prefabs\\gauge.xml".into(), loose: false });
    assert_eq!(ui.tile(ui.screen()).origin, Origin::Code);
    assert!(ui.mod_added_tiles(root).is_empty());
    assert!(ui.load_globals(&src));
    assert_eq!(ui.tile(ui.globals()).origin, Origin::File { path: "menus\\globals.xml".into(), loose: false });
}

#[test]
fn mod_roots_cover_injections_templates_and_loose_includes_once() {
    let src = MemSource::default()
        .with(
            "menu.xml",
            r#"<menu name="M"><rect name="File"><include src="loose.xml"/>
          <template name="row"><rect name="Row"><include src="child.xml"/></rect><rect name="Second"/></template>
        </rect><rect name="Other"/></menu>"#,
        )
        .with_loose("menus\\prefabs\\loose.xml", r#"<rect name="Loose"><include src="child.xml"/></rect>"#)
        .with("menus\\prefabs\\child.xml", r#"<text name="Child"><rect name="Grandchild"/></text>"#)
        .with("inject.xml", r#"<rect name="Injected"><include src="loose.xml"/></rect>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "menu.xml").unwrap();
    let loose = ui.find_tile("M/File/Loose").unwrap();
    assert_eq!(ui.tile(loose).origin, Origin::File { path: "menus\\prefabs\\loose.xml".into(), loose: true });
    for path in ["M/File/Loose/Child", "M/File/Loose/Child/Grandchild"] {
        assert_eq!(ui.tile(ui.find_tile(path).unwrap()).origin, Origin::File { path: "menus\\prefabs\\child.xml".into(), loose: false });
    }
    assert_eq!(ui.mod_added_tiles(root), [loose]);
    assert!(ui.has_mod_added_tiles(root));
    assert!(ui.inject_file(&src, "M/File", "Data/inject.xml"));
    let injected = ui.find_tile("M/File/Injected").unwrap();
    for path in ["M/File/Injected", "M/File/Injected/Loose", "M/File/Injected/Loose/Child/Grandchild"] {
        assert_eq!(ui.tile(ui.find_tile(path).unwrap()).origin, Origin::Injected);
    }
    let template = ui.add_from_template(&src, "M/File", "row", Some("ModRow")).unwrap();
    let second = ui.find_tile("M/File/Second").unwrap();
    for path in ["M/File/ModRow", "M/File/ModRow/Child", "M/File/ModRow/Child/Grandchild", "M/File/Second"] {
        assert_eq!(ui.tile(ui.find_tile(path).unwrap()).origin, Origin::Template);
    }
    let code = ui.add_code_tile_from_template(&src, "M/File", "row", Some("CodeRow")).unwrap();
    let file = ui.find_tile("M/File").unwrap();
    let code_second = *ui.tile(file).children.last().unwrap();
    assert_eq!(ui.tile(code_second).origin, Origin::Code);
    assert_eq!(ui.tile(code).origin, Origin::Code);
    let child = ui.find_tile("M/File/CodeRow/Child/Grandchild").unwrap();
    assert_eq!(ui.tile(child).origin, Origin::Code);
    assert!(ui.inject_text(&src, "M/File/CodeRow", r#"<image name="ModUnderCode"/>"#));
    let nested = ui.find_tile("M/File/CodeRow/ModUnderCode").unwrap();
    assert_eq!(ui.mod_added_tiles(root), [loose, injected, template, second, nested]);
    ui.unload_tile(injected);
    assert_eq!(ui.mod_added_tiles(root), [loose, template, second, nested]);
    ui.set_trait(file, "x", Value::Num(5.0));
    assert_eq!(ui.tile(file).origin, Origin::File { path: "menu.xml".into(), loose: false });
    ui.unload_tile(root);
    assert!(ui.mod_added_tiles(root).is_empty());
    assert!(!ui.has_mod_added_tiles(root));
}

#[test]
fn the_mod_tile_answer_follows_injection_templates_and_unloading() {
    // [design: #938] `has_mod_added_tiles` answers as `mod_added_tiles` walks, per menu.
    let src = MemSource::default()
        .with("hud.xml", r#"<menu name="HUD"><rect name="Piece"><template name="row"><rect name="Row"><rect name="Inner"/></rect></template></rect></menu>"#)
        .with("start.xml", r#"<menu name="Start"><rect name="Panel"/></menu>"#);
    let mut ui = Ui::new();
    let hud = ui.load_menu(&src, "hud.xml").unwrap();
    let start = ui.load_menu(&src, "start.xml").unwrap();
    let agree = |ui: &Ui| {
        for menu in [hud, start] {
            assert_eq!(ui.has_mod_added_tiles(menu), !ui.mod_added_tiles(menu).is_empty());
        }
    };
    assert!(!ui.has_mod_added_tiles(hud));
    agree(&ui);
    // A tile injected into another menu leaves the HUD's answer alone.
    assert!(ui.inject_text(&src, "Start/Panel", r#"<rect name="MCM"><text name="Label"/></rect>"#));
    assert!(ui.has_mod_added_tiles(start) && !ui.has_mod_added_tiles(hud));
    agree(&ui);
    // Injected with children: yes; the tree worked out leaves it so.
    assert!(ui.inject_text(&src, "HUD/Piece", r#"<rect name="Gauge"><image name="Bar"/></rect>"#));
    assert!(ui.has_mod_added_tiles(hud));
    ui.update();
    ui.set_trait(ui.find_tile("HUD/Piece").unwrap(), "x", Value::Num(3.0));
    ui.update();
    assert!(ui.has_mod_added_tiles(hud));
    agree(&ui);
    // A child of the injected tile unloaded: still yes; the injected tile unloaded: no.
    ui.unload_tile(ui.find_tile("HUD/Piece/Gauge/Bar").unwrap());
    assert!(ui.has_mod_added_tiles(hud));
    ui.unload_tile(ui.find_tile("HUD/Piece/Gauge").unwrap());
    assert!(!ui.has_mod_added_tiles(hud));
    agree(&ui);
    // A mod's template copy counts; the engine's own copies do not.
    ui.add_code_tile_from_template(&src, "HUD/Piece", "row", Some("CodeRow")).unwrap();
    assert!(!ui.has_mod_added_tiles(hud));
    let row = ui.add_from_template(&src, "HUD/Piece", "row", Some("ModRow")).unwrap();
    assert!(ui.has_mod_added_tiles(hud));
    agree(&ui);
    ui.unload_tile(row);
    assert!(!ui.has_mod_added_tiles(hud));
    // The menu read again starts with none; unloading a menu forgets its count.
    assert!(ui.inject_text(&src, "HUD", r#"<rect name="Again"/>"#));
    let hud = ui.load_menu(&src, "hud.xml").unwrap();
    assert!(!ui.has_mod_added_tiles(hud));
    ui.unload_tile(start);
    assert!(!ui.has_mod_added_tiles(start));
}

#[test]
fn a_loose_menu_is_one_mod_root_and_other_menus_are_excluded() {
    let src = MemSource::default().with_loose("Menus/M.xml", r#"<menu name="M"><rect name="Child"/></menu>"#).with("n.xml", r#"<menu name="N"/>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "Menus/M.xml").unwrap();
    let other = ui.load_menu(&src, "n.xml").unwrap();
    assert!(ui.inject_text(&src, "M/Child", r#"<image name="Extra"/>"#));
    assert!(ui.inject_text(&src, "N", r#"<image name="Other"/>"#));
    assert_eq!(ui.mod_added_tiles(root), [root]);
    assert_eq!(ui.mod_added_tiles(other), [ui.find_tile("N/Other").unwrap()]);
    assert!(!MemSource::default().is_loose("absent.xml"));
}

#[test]
fn a_loose_replacement_draws_only_what_the_archived_file_lacks() {
    // [design] The loose file keeps the archived tiles (and includes an archived prefab)
    // and adds one of its own; an injection and a code copy sit inside it.
    let archived = MemSource::default()
        .with("menus/m.xml", r#"<menu name="M"><rect name="Piece"><include src="p.xml"/></rect></menu>"#)
        .with("menus/prefabs/p.xml", r#"<text name="Label"/>"#);
    let loose = MemSource::default()
        .with_loose("menus/m.xml", r#"<menu name="M"><rect name="Piece"><include src="p.xml"/><image name="ModBar"/></rect><rect name="ModPanel"><include src="p.xml"/></rect></menu>"#)
        .with("menus/prefabs/p.xml", r#"<text name="Label"/>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&loose, "menus/m.xml").unwrap();
    assert!(ui.inject_text(&loose, "M/Piece", r#"<rect name="Injected"/>"#));
    assert!(ui.inject_text(&loose, "M", r#"<template name="row"><rect name="Row"/></template>"#));
    let code = ui.add_code_tile_from_template(&loose, "M", "row", Some("CodeRow")).unwrap();
    assert_eq!(ui.mod_added_tiles(root), [root]);
    let vanilla = Ui::file_name_paths(&archived, "menus/m.xml").unwrap();
    assert_eq!(vanilla.len(), 3);
    let drawn = |names: Option<&std::collections::HashSet<String>>| -> Vec<String> {
        let mut out: Vec<String> = ui.name_paths(root).into_iter().filter(|p| ui.mod_tile_drawn(ui.find_tile(p).unwrap(), names)).collect();
        out.sort();
        out
    };
    assert_eq!(drawn(Some(&vanilla)), ["m/modpanel", "m/modpanel/label", "m/piece/injected", "m/piece/modbar"]);
    // Without the archived file's names only the injection is drawn.
    assert_eq!(drawn(None), ["m/piece/injected"]);
    assert!(!ui.mod_tile_drawn(code, Some(&vanilla)));
    assert!(Ui::file_name_paths(&MemSource::default(), "menus/m.xml").is_none());
}

#[test]
fn the_archived_copy_under_a_loose_file_gives_the_names_compared_with() {
    // [design] One source holds both copies of the menu file (as the VFS does since #975);
    // the archived one includes a prefab a mod also replaced loose.
    let src = MemSource::default()
        .with_loose_over(
            "menus/m.xml",
            r#"<menu name="M"><rect name="Piece"><include src="p.xml"/><image name="ModBar"/></rect></menu>"#,
            r#"<menu name="M"><rect name="Piece"><include src="p.xml"/></rect></menu>"#,
        )
        .with_loose_over("menus/prefabs/p.xml", r#"<text name="Label"/><text name="ModLabel"/>"#, r#"<text name="Label"/>"#)
        .with("menus/other.xml", r#"<menu name="Other"/>"#)
        .with_loose("menus/new.xml", r#"<menu name="New"/>"#);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "menus/m.xml").unwrap();
    assert_eq!(ui.mod_added_tiles(root), [root]);
    let vanilla = Ui::archived_name_paths(&src, "menus/m.xml").unwrap();
    let mut expected: Vec<&str> = vec!["m", "m/piece", "m/piece/label"];
    let mut got: Vec<&str> = vanilla.iter().map(String::as_str).collect();
    got.sort();
    expected.sort();
    assert_eq!(got, expected);
    let mut drawn: Vec<String> = ui.name_paths(root).into_iter().filter(|p| ui.mod_tile_drawn(ui.find_tile(p).unwrap(), Some(&vanilla))).collect();
    drawn.sort();
    assert_eq!(drawn, ["m/piece/modbar", "m/piece/modlabel"]);
    // A file that is not loose reads as it wins; a loose file with no archive under it
    // has no archived copy.
    assert_eq!(src.read_archived("Menus/Other.xml"), src.read("menus/other.xml"));
    assert!(src.read_archived("menus/new.xml").is_none());
    assert!(Ui::archived_name_paths(&src, "menus/new.xml").is_none());
}

#[test]
fn a_trait_knows_it_holds_a_built_in_label() {
    let xml = r#"<menu name="M"><rect name="Label"><_T>&-sNvfLabel;</_T></rect><rect name="Plugin"><_T>&-sNvfPlugin;</_T></rect><rect name="Literal"><_T>word</_T></rect><rect name="Mixed"><_T>&-sNvfLabel; more</_T></rect></menu>"#;
    let mut src = MemSource::default().with("menus/l.xml", xml);
    src.strings.insert("sNvfLabel".into(), "word".into());
    src.strings.insert("sNvfPlugin".into(), "word".into());
    src.built_in_strings.insert("snvflabel".into());
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/l.xml").unwrap();
    let label = |ui: &Ui, name: &str| ui.built_in_label(ui.find_tile(&format!("M/{name}")).unwrap(), "_T").map(str::to_owned);
    assert_eq!(label(&ui, "Label").as_deref(), Some("snvflabel"));
    assert_eq!(label(&ui, "Plugin"), None, "the same words from a plugin");
    assert_eq!(label(&ui, "Literal"), None, "the same words from a file");
    assert_eq!(label(&ui, "Mixed"), None);
    let t = ui.find_tile("M/Label").unwrap();
    ui.set_trait(t, "_T", Value::Str("word".into()));
    assert_eq!(label(&ui, "Label"), None, "a script's write");
}

#[test]
fn game_strings_resolve_from_the_load_order_then_the_built_in_table() {
    // menu-xml.md §2.4 (T10): `&-sName;` reads the load order's GMST, then the built-in
    // table; a name in neither reads empty. Synthetic names and text only.
    struct Files {
        mem: MemSource,
        strings: crate::GameStrings,
    }
    impl Source for Files {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.mem.read(path)
        }
        fn game_string(&self, name: &str) -> Option<String> {
            self.strings.get(name)
        }
    }
    const BUILT_IN: &[crate::strings::Entry] = &[("sNvfButton", "built-in button", "test"), ("sNvfTitle", "built-in title", "test")];
    let mem = MemSource::default().with(
        "menus/s.xml",
        r#"<menu name="S">
  <text name="Title"> <string> &-SNVFTITLE; </string> </text>
  <text name="Button"> <string> &-sNvfButton; </string> </text>
  <text name="Missing"> <string> &-sNvfMissing; </string> </text>
  <text name="Copied"> <string> <copy> &-sNvfButton; </copy> </string> </text>
</menu>"#,
    );
    let src = Files { mem, strings: crate::GameStrings::with_built_in([("sNvfTitle", "first plugin"), ("sNvfTitle", "last plugin")], BUILT_IN) };
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/s.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_string("S/Title/string").as_deref(), Some("last plugin"));
    assert_eq!(ui.get_string("S/Button/string").as_deref(), Some("built-in button"));
    assert_eq!(ui.get_string("S/Missing/string").as_deref(), Some(""));
    assert_eq!(ui.get_string("S/Copied/string").as_deref(), Some("built-in button"));
    let read: Vec<_> = ui.game_strings_read.iter().map(|(n, &found)| (n.as_str(), found)).collect();
    assert_eq!(read, [("snvfbutton", true), ("snvfmissing", false), ("snvftitle", true)]);
}

#[test]
fn mem_source_game_strings_ignore_case_and_unknown_names_read_empty() {
    // [observed: menu-xml.md §2.4] lookup ignores case; an unknown name is empty text.
    let mut src = MemSource::default().with("menus/c.xml", r#"<menu name="C"><text name="A"><string>&-SNVFCASE;</string></text><text name="B"><string>&-sNvfUnknown;</string></text></menu>"#);
    src.strings.insert("sNvfCase".into(), "case text".into());
    assert_eq!(src.game_string("snvfcase").as_deref(), Some("case text"));
    assert_eq!(src.game_string("sNvfUnknown"), None);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/c.xml").unwrap();
    assert_eq!(ui.get_string("C/A/string").as_deref(), Some("case text"));
    assert_eq!(ui.get_string("C/B/string").as_deref(), Some(""));
}

/// Every live tile's traits by path: the pseudo-tiles' and the menus'.
fn all_values(ui: &Ui) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<_> = [ui.screen(), ui.strings(), ui.globals()].into_iter().chain(ui.menus()).collect();
    while let Some(id) = stack.pop() {
        for t in &ui.tile(id).traits {
            out.push((ui.path_of(id), t.name.clone(), format!("{:?}", t.value)));
        }
        stack.extend(ui.tile(id).children.iter().copied());
    }
    out.sort();
    out
}

#[test]
fn reopening_a_menu_leaves_the_tree_its_size() {
    // [design: #1039] Unloaded tiles' slots are used again.
    let src = files();
    let mut ui = loaded();
    let (slots, live) = (ui.tile_slots(), ui.live_tiles());
    let before = all_values(&ui);
    for _ in 0..50 {
        let menu = ui.menu("TestMenu").unwrap();
        ui.unload_tile(menu);
        ui.update();
        assert!(ui.live_tiles() < live);
        ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
        ui.update();
        // Loading over the loaded menu frees the old one first, too.
        ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
        ui.update();
    }
    assert_eq!((ui.tile_slots(), ui.live_tiles()), (slots, live));
    assert_eq!(all_values(&ui), before);
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:0/Bar/width"), Some(380.0));
}

#[test]
fn templates_and_injection_work_in_a_reopened_menu() {
    let src = files();
    let mut ui = loaded();
    let made = ui.add_from_template(&src, "TestMenu/Panel", "row", Some("Row0")).unwrap();
    let menu = ui.menu("TestMenu").unwrap();
    ui.unload_tile(menu);
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    // The reopened menu has no Row0 (it was made in the old one) and makes its own.
    assert!(ui.find_tile("TestMenu/Panel/Row0").is_none());
    assert!(!ui.is_live(made));
    let again = ui.add_from_template(&src, "TestMenu/Panel", "row", Some("Row0")).unwrap();
    assert_ne!(again, made);
    assert_eq!(ui.path_of(again), "TestMenu/Panel/Row0");
    assert!(ui.inject_text(&src, "TestMenu/Panel", r#"<text name="Added"> <x> <copy src="parent()" trait="width"/> </x> </text>"#));
    ui.update();
    assert_eq!(ui.get_float("TestMenu/Panel/Added/x"), Some(400.0));
    assert_eq!(ui.get_string("TestMenu/Panel/Row0/Label/string").as_deref(), Some("row"));
    let root = ui.menu("TestMenu").unwrap();
    assert_eq!(ui.mod_added_tiles(root).len(), 2);
    assert!(ui.has_mod_added_tiles(root));
}

#[test]
fn uio_includes_apply_again_in_a_reopened_menu() {
    let src = MemSource::default()
        .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Existing"/></menu>"#)
        .with_loose("menus/prefabs/added.xml", r#"<rect name="Added"><text name="Child"/></rect>"#)
        .with("uio/public/test.txt", "added.xml::HUDMainMenu::Existing\ntrue");
    let mut ui = Ui::new();
    ui.configure_uio(&src, |_| false);
    let first = ui.load_menu(&src, "menus/hud.xml").unwrap();
    let old_added = ui.find_tile("HUDMainMenu/Existing/Added").unwrap();
    assert_eq!(ui.mod_added_tiles(first), [old_added]);
    let slots = ui.tile_slots();
    for _ in 0..5 {
        let root = ui.menu("HUDMainMenu").unwrap();
        ui.unload_tile(root);
        assert!(!ui.has_mod_added_tiles(root));
        ui.load_menu(&src, "menus/hud.xml").unwrap();
    }
    let root = ui.menu("HUDMainMenu").unwrap();
    let added = ui.find_tile("HUDMainMenu/Existing/Added").unwrap();
    assert_ne!(added, old_added);
    assert_eq!(ui.mod_added_tiles(root), [added]);
    assert!(ui.has_mod_added_tiles(root));
    assert!(ui.mod_tile_drawn(added, None) && ui.mod_tile_drawn(ui.tile(added).children[0], None));
    // A stale UIO tile's id is no mod tile, though a new tile has its slot.
    assert!(!ui.mod_tile_drawn(old_added, None));
    assert_eq!(ui.tile_slots(), slots);
    assert_eq!(ui.uio.decisions[0].applied, 6);
}

#[test]
fn stale_ids_name_no_tile_and_never_a_new_one() {
    let src = files();
    let mut ui = loaded();
    let old_menu = ui.menu("TestMenu").unwrap();
    let old_title = ui.find_tile("TestMenu/Panel/Title").unwrap();
    ui.write_changed(old_title, "_code", Value::Num(1.0));
    ui.unload_tile(old_menu);
    ui.load_menu(&src, "menus\\test_menu.xml").unwrap();
    ui.update();
    let menu = ui.menu("TestMenu").unwrap();
    let title = ui.find_tile("TestMenu/Panel/Title").unwrap();
    // The new tiles took old slots, under new ids.
    assert!(menu.index() < ui.tile_slots() as u32 && title.index() < ui.tile_slots() as u32);
    assert_ne!(menu, old_menu);
    assert_ne!(title, old_title);
    for stale in [old_menu, old_title] {
        assert!(!ui.is_live(stale));
        let t = ui.tile(stale);
        assert!(!t.live && t.name.is_empty() && t.children.is_empty() && t.traits.is_empty());
        assert_eq!(ui.path_of(stale), "");
        assert!(ui.mod_added_tiles(stale).is_empty() && !ui.has_mod_added_tiles(stale));
    }
    // Writes and unloads through a stale id do nothing to the new tiles.
    let before = all_values(&ui);
    ui.set_trait(old_title, "x", Value::Num(99.0));
    ui.write_changed(old_title, "_code", Value::Num(2.0));
    ui.unload_tile(old_menu);
    ui.unload_tile(old_title);
    ui.update();
    assert_eq!(all_values(&ui), before);
    assert!(ui.is_live(menu) && ui.is_live(title));
    // What the code last wrote was the old tile's: the new tile takes the same value.
    ui.write_changed(title, "_code", Value::Num(1.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Title/_code"), Some(1.0));
}

/// What `update` worked out (not skipped).
fn evaluations(ui: &mut Ui) -> usize {
    ui.evaluations = 0;
    ui.update();
    ui.evaluations
}

#[test]
fn an_update_skips_expressions_whose_sources_did_not_change() {
    let src = files();
    let mut ui = loaded();
    // Values nothing reads (the first Gauge's _Fill is 0, so its Bar reads `_pic_0`):
    // nothing worked out.
    assert!(ui.set_string("TestMenu/Panel/Gauge/Bar/_pic_1", "other.dds"));
    assert!(ui.set_string("TestMenu/Panel/Title/string", "other"));
    assert_eq!(evaluations(&mut ui), 0);
    // `_Selected` is read by Title's alpha and visible only.
    assert!(ui.set_float("TestMenu/_Selected", 1.0));
    assert_eq!(evaluations(&mut ui), 2);
    assert_eq!(ui.get_float("TestMenu/Panel/Title/alpha"), Some(295.0));
    // A source read through a chain: the first Gauge's _Fill, its Bar's width, then the
    // Bar's x (which reads that width) and filename.
    assert!(ui.set_float("TestMenu/Panel/Gauge:1/_Fill", 0.25));
    assert_eq!(evaluations(&mut ui), 3);
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:1/Bar/width"), Some(95.0));
    assert_eq!(ui.get_float("TestMenu/Panel/Gauge:1/Bar/x"), Some(-47.5));
    // A new tile changes the shape: everything is worked out once, then skipped again.
    assert!(ui.inject_text(&src, "TestMenu", "<rect name=\"New\"/>"));
    // Panel's height, two Gauges' width and their Bars' width, x and filename, Title's x,
    // alpha and visible.
    assert_eq!(evaluations(&mut ui), 12);
    assert_eq!(evaluations(&mut ui), 0);
    assert!(ui.set_float("TestMenu/_Selected", 0.0));
    assert_eq!(evaluations(&mut ui), 2);
}

#[test]
fn skipping_finds_what_working_everything_out_finds() {
    // [design: #1039] The same writes to a tree that skips and to one that never does.
    let mut fast = loaded();
    let mut slow = loaded();
    slow.never_skip = true;
    let steps: [fn(&mut Ui); 16] = [
        |ui| {
            let s = ui.screen();
            ui.set_trait(s, "height", Value::Num(500.0));
        },
        |ui| _ = ui.set_float("TestMenu/_Selected", 1.0),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge/_Fill", 0.25),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge:0/_Fill", 0.0),
        |ui| _ = ui.set_float("TestMenu/Panel/Title/x", 7.0),
        |ui| _ = ui.mod_float("TestMenu/Panel/width", 100.0),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge/width", 100.0),
        |ui| _ = ui.inject_text(&files(), "TestMenu/Panel", "<rect name=\"Gauge\"> <_Fill> 0.75 </_Fill> <include src=\"gauge.xml\"/> </rect>"),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge:0/_Fill", 1.0),
        |ui| _ = ui.set_string("TestMenu/Panel/Gauge:1/Bar/_pic_1", "other.dds"),
        |ui| _ = ui.set_float("TestMenu/Panel/Gauge:1/_Fill", 0.1),
        |ui| {
            let g = ui.find_tile("TestMenu/Panel/Gauge:2").unwrap();
            ui.unload_tile(g);
        },
        |ui| _ = ui.set_float("TestMenu/Panel/width", 300.0),
        |ui| _ = ui.add_from_template(&files(), "TestMenu/Panel", "row", Some("Row0")),
        |ui| _ = ui.load_menu(&files(), "menus\\test_menu.xml"),
        |ui| _ = ui.set_float("TestMenu/_Selected", 0.0),
    ];
    for (n, step) in steps.iter().enumerate() {
        step(&mut fast);
        step(&mut slow);
        assert_eq!(fast.update(), slow.update(), "step {n}");
        assert_eq!(all_values(&fast), all_values(&slow), "step {n}");
    }
}

#[test]
fn ids_from_a_replaced_tree_name_no_tile_of_the_new_one() {
    // [design: #1061] Two trees built alike have their tiles in the same slots.
    let old = loaded();
    let mut new = loaded();
    let old_title = old.find_tile("TestMenu/Panel/Title").unwrap();
    let title = new.find_tile("TestMenu/Panel/Title").unwrap();
    assert_eq!(old_title.index(), title.index());
    assert_ne!(old_title, title);
    assert!(!new.is_live(old_title) && !new.tile(old_title).live);
    assert!(!new.is_live(old.screen()));
    let before = all_values(&new);
    new.set_trait(old_title, "x", Value::Num(99.0));
    new.unload_tile(old.menu("TestMenu").unwrap());
    new.update();
    assert_eq!(all_values(&new), before);
}

#[test]
fn a_shape_changed_before_every_update_is_told_once() {
    // [design: #1061] Values written every update: no warning.
    let mut ui = loaded();
    for n in 0..300 {
        ui.set_float("TestMenu/_Fill", n as f64);
        ui.update();
        assert_eq!(ui.take_reshape_warning(), None);
    }
    // A new trait every update (a shape change each time) is told once, naming the call.
    let s = ui.screen();
    let mut told = Vec::new();
    for n in 0..300 {
        ui.set_trait(s, &format!("_made{n}"), Value::Num(1.0));
        ui.update();
        told.extend(ui.take_reshape_warning());
    }
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("set_trait called at") && told[0].contains("tests.rs"), "{}", told[0]);
}

#[test]
fn a_nested_loose_uio_include_counts_every_tile_it_adds() {
    // [design: #1061] The loose include includes an archived file, which includes another.
    let src = MemSource::default()
        .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Existing"><rect name="Own"/></rect></menu>"#)
        .with_loose("menus/prefabs/added.xml", r#"<rect name="Added"><include src="child.xml"/></rect><include src="sibling.xml"/>"#)
        .with("menus/prefabs/child.xml", r#"<text name="Child"><include src="grand.xml"/></text>"#)
        .with("menus/prefabs/grand.xml", r#"<image name="Grand"/>"#)
        .with("menus/prefabs/sibling.xml", r#"<rect name="Sibling"><include src="grand.xml"/></rect>"#)
        .with("uio/public/test.txt", "added.xml::HUDMainMenu::Existing\ntrue");
    let mut ui = Ui::new();
    ui.configure_uio(&src, |_| false);
    let root = ui.load_menu(&src, "menus/hud.xml").unwrap();
    let added = ["Existing/Added", "Existing/Added/Child", "Existing/Added/Child/Grand", "Existing/Sibling", "Existing/Sibling/Grand"]
        .map(|p| ui.find_tile(&format!("HUDMainMenu/{p}")).unwrap());
    for t in added {
        assert!(ui.mod_tile_drawn(t, None), "{}", ui.path_of(t));
    }
    // The menu's own tiles are not the mod's.
    assert!(!ui.mod_tile_drawn(ui.find_tile("HUDMainMenu/Existing/Own").unwrap(), None));
    assert_eq!(ui.mod_added_tiles(root), [added[0], added[3]]);
    assert!(ui.has_mod_added_tiles(root));
    ui.unload_tile(added[0]);
    assert!(ui.has_mod_added_tiles(root));
    ui.unload_tile(added[3]);
    assert!(!ui.has_mod_added_tiles(root));
}

#[test]
fn a_reopened_deep_menu_settles_in_as_few_passes_as_the_first_time() {
    // [design: review of #1045] Each tile's x reads its parent's: 20 levels deep. Slots
    // used again must keep parents before children, as `update` walks them.
    let mut deep = String::from(r#"<menu name="Deep"> <x> 5 </x>"#);
    for i in 0..20 {
        deep.push_str(&format!(r#"<rect name="L{i}"> <x> <copy src="parent()" trait="x"/> <add> 1 </add> </x>"#));
    }
    deep.push_str(&"</rect>".repeat(20));
    deep.push_str("</menu>");
    let src = MemSource::default()
        .with("menus/deep.xml", &deep)
        .with("menus/small.xml", r#"<menu name="Small"> <rect name="A"/> <rect name="B"> <rect name="C"/> </rect> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/small.xml").unwrap();
    ui.load_menu(&src, "menus/deep.xml").unwrap();
    ui.update();
    let first = ui.passes;
    assert!(first <= 3, "{first}");
    let path = format!("Deep/{}/x", (0..20).map(|i| format!("L{i}")).collect::<Vec<_>>().join("/"));
    assert_eq!(ui.get_float(&path), Some(25.0));
    for _ in 0..5 {
        // Both menus' slots are free, in the order they were unloaded.
        let small = ui.menu("Small").unwrap();
        let deep = ui.menu("Deep").unwrap();
        ui.unload_tile(deep);
        ui.unload_tile(small);
        ui.update();
        ui.load_menu(&src, "menus/deep.xml").unwrap();
        ui.update();
        assert_eq!(ui.passes, first);
        assert_eq!(ui.get_float(&path), Some(25.0));
        ui.load_menu(&src, "menus/small.xml").unwrap();
        ui.update();
    }
    assert!(ui.set_float("Deep/x", 0.0));
    ui.update();
    assert_eq!(ui.passes, first);
    assert_eq!(ui.get_float(&path), Some(20.0));
}

#[test]
fn a_trait_by_a_path_under_a_tile() {
    let src = MemSource::default().with("menus/m9.xml", r#"<menu name="M"> <rect name="Item"> <_y> 4 </_y> <text name="Label"> <string> a </string> </text> </rect> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/m9.xml").unwrap();
    let item = ui.find_tile("M/Item").unwrap();
    let (t, k) = ui.find_trait_in(item, "_y").unwrap();
    assert_eq!((t, ui.tile(t).traits[k].value.num()), (item, 4.0));
    let (t, k) = ui.find_trait_in(item, "Label/string").unwrap();
    assert_eq!(ui.tile(t).traits[k].value.text(), "a");
    assert!(ui.find_trait_in(item, "Label/nope").is_none());
    assert!(ui.find_trait_in(item, "").is_none());
}

#[test]
fn a_trait_name_ending_in_an_underscore_after_a_constant_reads_its_number() {
    // [design] The fade-line and meter prefabs' shape: a `&false;`/`&true;` flag copied
    // before `copy … trait="_pic_"` (menu-xml.md §4.4, T15a).
    let src = MemSource::default().with(
        "menus/flag.xml",
        r#"<menu name="Flags">
  <image name="Off"> <_flag> &false; </_flag> <_pic_0> off.dds </_pic_0> <_pic_1> on.dds </_pic_1>
    <filename> <copy src="me()" trait="_flag"/> <copy src="me()" trait="_pic_"/> </filename> </image>
  <image name="On"> <_flag> &true; </_flag> <_pic_0> off.dds </_pic_0> <_pic_1> on.dds </_pic_1>
    <filename> <copy src="me()" trait="_flag"/> <copy src="me()" trait="_pic_"/> </filename> </image>
  <image name="Unknown"> <_flag> &nvfunknown; </_flag> <_pic_0> off.dds </_pic_0>
    <filename> <copy src="me()" trait="_flag"/> <copy src="me()" trait="_pic_"/> </filename> </image>
</menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/flag.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_string("Flags/Off/filename").as_deref(), Some("off.dds"));
    assert_eq!(ui.get_string("Flags/On/filename").as_deref(), Some("on.dds"));
    // A constant without a known number names no suffix: `_pic_` itself is read, and it
    // is absent, so the copy reads the default value (0, as a string "0").
    assert_eq!(ui.get_string("Flags/Unknown/filename").as_deref(), Some("0"));
}

#[test]
fn a_trait_taken_off_a_tile_reads_as_never_set() {
    let src = MemSource::default().with("m.xml", r#"<menu name="M"><rect name="R"><_read><copy>5</copy><add src="screen()" trait="cropy"/></_read></rect></menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("M/R/_read"), Some(5.0));
    let screen = ui.screen();
    assert!(!ui.remove_trait(screen, "cropy"), "nothing to take");
    ui.set_trait(screen, "cropy", Value::Num(15.0));
    ui.update();
    assert_eq!(ui.get_float("M/R/_read"), Some(20.0));
    assert!(ui.remove_trait(screen, "cropy"));
    assert!(ui.tile(screen).get("cropy").is_none());
    ui.update();
    assert_eq!(ui.get_float("M/R/_read"), Some(5.0));
}

#[test]
fn traits_taken_off_one_after_another_each_tell_their_own_readers() {
    // Three traits on one tile and a reader of each, all worked out; the first two go with no
    // update between. Each reader of a gone trait reads nothing again; the third keeps reading
    // its trait, which moved down two places, and hears of its next change.
    let src = MemSource::default().with(
        "m.xml",
        r#"<menu name="M"><rect name="R">
          <_a><copy>1</copy><add src="screen()" trait="one"/></_a>
          <_b><copy>2</copy><add src="screen()" trait="two"/></_b>
          <_c><copy>3</copy><add src="screen()" trait="three"/></_c></rect></menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    let screen = ui.screen();
    for (name, v) in [("one", 10.0), ("two", 20.0), ("three", 30.0)] {
        ui.set_trait(screen, name, Value::Num(v));
    }
    ui.update();
    let read = |ui: &Ui| ["_a", "_b", "_c"].map(|n| ui.get_float(&format!("M/R/{n}")).unwrap());
    assert_eq!(read(&ui), [11.0, 22.0, 33.0]);
    assert!(ui.remove_trait(screen, "one"));
    assert!(ui.remove_trait(screen, "two"));
    ui.update();
    assert_eq!(read(&ui), [1.0, 2.0, 33.0]);
    ui.set_trait(screen, "three", Value::Num(40.0));
    ui.update();
    assert_eq!(read(&ui), [1.0, 2.0, 43.0]);
    // Back again, in the other order of going.
    ui.set_trait(screen, "one", Value::Num(5.0));
    ui.set_trait(screen, "two", Value::Num(6.0));
    ui.update();
    assert_eq!(read(&ui), [6.0, 8.0, 43.0]);
    assert!(ui.remove_trait(screen, "two"));
    assert!(ui.remove_trait(screen, "one"));
    assert!(ui.remove_trait(screen, "three"));
    ui.update();
    assert_eq!(read(&ui), [1.0, 2.0, 3.0]);
}

#[test]
fn trait_write_counts_tell_a_write_from_a_change() {
    // [design] Measuring what a frame writes (the engine's NVF_PLAY_TEST_TILE_COUNTS):
    // writing the same value again counts as a write only.
    let mut ui = Ui::new();
    let screen = ui.screen();
    let (writes, changes) = ui.trait_write_counts();
    ui.set_trait(screen, "width", Value::Num(1280.0));
    ui.set_trait(screen, "width", Value::Num(1280.0));
    ui.set_trait(screen, "width", Value::Num(1920.0));
    assert_eq!(ui.trait_write_counts(), (writes + 3, changes + 2));
}

#[test]
fn the_extenders_paths_count_from_the_first_child() {
    use crate::Count::FromFirst;
    // MCM's shape: a rect with its parts in file order, the options panel first with its
    // rows [documented: MCM Guide v6, `StartMenu/MCM/*:1/*:2` is Option2 of MCM_Options,
    // `*:9` MCM_Info; menu-xml.md §10.4].
    let src = MemSource::default().with(
        "m.xml",
        r#"<menu name="StartMenu"> <rect name="MCM">
            <rect name="MCM_Options"> <hotrect name="Option1"> <_value> 1 </_value> </hotrect>
              <hotrect name="Option2"> <_value> 2 </_value> </hotrect> <hotrect name="Option3"> <_value> 3 </_value> </hotrect> </rect>
            <rect name="MCM_Scale"> <_n> 2 </_n> </rect>
            <rect name="MCM_List"> <_n> 3 </_n> </rect>
        </rect> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "m.xml").unwrap();
    let get = |ui: &Ui, p: &str| ui.get_counted(p, FromFirst).map(Value::num);
    assert_eq!(get(&ui, "StartMenu/MCM/*:1/*:2/_value"), Some(2.0), "the first child's second");
    assert_eq!(get(&ui, "StartMenu/MCM/*:1/*:1/_value"), Some(1.0));
    assert_eq!(get(&ui, "StartMenu/MCM/*:2/_n"), Some(2.0));
    // 0 the last; none past the one before the last [observed: menu-xml.md §10.1].
    assert_eq!(get(&ui, "StartMenu/MCM/*:0/_n"), Some(3.0));
    assert_eq!(get(&ui, "StartMenu/MCM/*:1/*:0/_value"), Some(3.0));
    assert_eq!(get(&ui, "StartMenu/MCM/*:3/_n"), None);
    assert_eq!(get(&ui, "StartMenu/MCM/MCM_Options/Option3/_value"), Some(3.0), "names as ever");
    // JIP's count from the last stays.
    assert_eq!(ui.get_float("StartMenu/MCM/*:2/*:0/_value"), Some(3.0));
    assert_eq!(ui.get_float("StartMenu/MCM/*:2/*:1/_value"), Some(2.0));
    assert!(ui.set_counted("StartMenu/MCM/*:1/*:2/_value", FromFirst, Value::Num(0.0)));
    assert_eq!(ui.get_float("StartMenu/MCM/MCM_Options/Option2/_value"), Some(0.0));
    assert!(!ui.set_counted("StartMenu/MCM/*:7/_value", FromFirst, Value::Num(0.0)));
}

/// A trait name with `+` in it is one name [observed: Vanilla UI Plus's `_VUI+Fade`,
/// `_VUI+OverwriteVerify`; #2298].
#[test]
fn a_plus_in_a_trait_name_is_part_of_it() {
    let src = MemSource::default().with(
        "menus\\a.xml",
        r#"<menu name="A"> <_VUI+Fade> 2 </_VUI+Fade> <_VUI> 1 </_VUI> <_VUI+Check> <copy src="me" trait="_VUI+Fade"/> <add src="me" trait="_VUI"/> </_VUI+Check> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\a.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("A/_VUI+Fade"), Some(2.0));
    assert_eq!(ui.get_float("A/_VUI"), Some(1.0));
    assert_eq!(ui.get_float("A/_VUI+Check"), Some(3.0));
}

/// `sibling(name)` on a menu's root is the other menu of that name [observed: Vanilla UI
/// Plus's start menu reads `sibling(HUDMainMenu)`; #2298].
#[test]
fn a_menus_sibling_is_another_menu() {
    let src = MemSource::default()
        .with("menus/h.xml", r#"<menu name="HUDMainMenu"> <_VUI+> 1 </_VUI+> </menu>"#)
        .with("menus/s.xml", r#"<menu name="StartMenu"> <_VUI+> <copy src="sibling(HUDMainMenu)" trait="_VUI+"/> </_VUI+> </menu>"#);
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/s.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("StartMenu/_VUI+"), Some(0.0), "no HUD menu yet");
    ui.load_menu(&src, "menus/h.xml").unwrap();
    ui.update();
    assert_eq!(ui.get_float("StartMenu/_VUI+"), Some(1.0));
}

/// MCM's OptionRect: the value text stands left of the suffix by the suffix's width, which
/// the renderer writes after the suffix's string changed (issue #2298, "6seconds").
#[test]
fn a_measured_width_moves_what_reads_it() {
    let src = MemSource::default().with(
        "menus/m.xml",
        r#"<menu name="M"> <_suffix> 0 </_suffix> <_suffixText> </_suffixText>
             <rect name="value">
               <_xSuffix> <copy> 0 </copy> <sub src="child(suffix)" trait="width"/> <onlyif src="parent()" trait="_suffix"/> </_xSuffix>
               <text name="text"> <x> <copy src="parent()" trait="_xSuffix"/> </x> </text>
               <text name="suffix"> <string> <copy src="grandparent()" trait="_suffixText"/> </string> <visible> <copy src="grandparent()" trait="_suffix"/> </visible> </text>
             </rect> </menu>"#,
    );
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus/m.xml").unwrap();
    ui.update();
    let suffix = ui.find_tile("M/value/suffix").unwrap();
    ui.set_trait(suffix, "width", Value::Num(0.0));
    ui.update();
    // The script: the suffix and its text, then a frame's update.
    ui.set_float("M/_suffix", 1.0);
    ui.set_string("M/_suffixText", " seconds");
    ui.update();
    assert_eq!(ui.get_float("M/value/text/x"), Some(0.0));
    // The next frame's measure, then its update.
    ui.set_trait(suffix, "width", Value::Num(80.0));
    ui.update();
    assert_eq!(ui.get_float("M/value/text/x"), Some(-80.0));
}

/// Independently authored plain/derived values, includes and a template; no game XML.
fn serial_files() -> MemSource {
    MemSource::default()
        .with("menus\\serial.xml", r#"<menu name="SerialMenu">
            <_source>4</_source><_n>12</_n><_word>same</_word>
            <_constant><copy>12</copy></_constant>
            <_computed><copy src="me()" trait="_source"/><add>1</add></_computed>
            <image name="First"><filename>owned.dds</filename><x>12</x></image>
            <image name="Second"><filename>other.dds</filename><x>24</x></image>
            <template name="row"><rect name="Made"><include src="serial_value.xml"/></rect></template>
        </menu>"#)
        .with("menus\\prefabs\\serial_value.xml", "<_n>12</_n>")
}

fn serial_tree() -> (Ui, crate::tree::TileId) {
    let mut ui = Ui::new();
    let root = ui.load_menu(&serial_files(), "menus\\serial.xml").unwrap();
    ui.update();
    (ui, root)
}

#[test]
fn trait_write_serial_equal_values_and_each_setter() {
    let (mut ui, root) = serial_tree();
    let initial_changed = ui.tile(root).traits[ui.tile(root).trait_index("_n").unwrap()].changed;
    let counts = ui.trait_write_counts();
    let writes: [fn(&mut Ui, crate::tree::TileId); 5] = [
        |ui, _| assert!(ui.set("SerialMenu/_n", Value::Num(12.0))),
        |ui, _| assert!(ui.set_float("SerialMenu/_n", 12.0)),
        |ui, t| assert!(ui.set_existing(t, "_n", Value::Num(12.0))),
        |ui, _| assert!(ui.mod_float("SerialMenu/_n", 0.0)),
        |ui, t| ui.set_trait(t, "_n", Value::Num(12.0)),
    ];
    let mut last = ui.trait_write_serial(root, "_n").unwrap();
    for write in writes {
        write(&mut ui, root);
        let now = ui.trait_write_serial(root, "_n").unwrap();
        assert_ne!(now, last);
        last = now;
    }
    let word = ui.trait_write_serial(root, "_word").unwrap();
    assert!(ui.set_string("SerialMenu/_word", "same"));
    assert_ne!(ui.trait_write_serial(root, "_word"), Some(word));
    assert_eq!(ui.tile(root).traits[ui.tile(root).trait_index("_n").unwrap()].changed, initial_changed);
    assert_eq!(ui.trait_write_counts(), (counts.0 + 6, counts.1));
    assert!(!ui.dirty());
    assert_eq!(ui.update(), 0);
    assert!(!ui.set_float("SerialMenu/missing", 1.0));
    assert!(!ui.set_existing(root, "missing", Value::Num(1.0)));
    assert_eq!(ui.trait_write_serial(root, "missing"), None);
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(last));
    ui.set_trait(root, "_n", Value::Num(13.0));
    ui.set_trait(root, "_n", Value::Num(12.0));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(last), "changing away and back never restores a token");
}

#[test]
fn trait_write_serial_unrelated_writes_and_case_aliases() {
    let (mut ui, root) = serial_tree();
    let first = ui.find_tile("SerialMenu/First").unwrap();
    let second = ui.find_tile("SerialMenu/Second").unwrap();
    let stamp = ui.trait_write_serial(first, "filename").unwrap();
    ui.set_trait(second, "filename", Value::Str("changed.dds".into()));
    ui.set_trait(first, "x", Value::Num(13.0));
    assert_eq!(ui.trait_write_serial(first, "FILENAME"), Some(stamp));
    ui.set_trait(first, "FiLeNaMe", Value::Str("owned.dds".into()));
    assert_ne!(ui.trait_write_serial(first, "filename"), Some(stamp));
    assert!(ui.trait_write_serial(root, "_n").is_some());
}

#[test]
fn trait_write_serial_changed_only_wrapper_keeps_no_write_semantics() {
    let (mut ui, root) = serial_tree();
    let original = ui.trait_write_serial(root, "_n").unwrap();
    ui.write_changed(root, "_n", Value::Num(12.0));
    let first = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(first, original, "first accepted code write counts even when equal");
    let counts = ui.trait_write_counts();
    ui.write_changed(root, "_n", Value::Num(12.0));
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(first));
    assert_eq!(ui.trait_write_counts(), counts);
    ui.set_trait(root, "_n", Value::Num(20.0));
    let external = ui.trait_write_serial(root, "_n").unwrap();
    ui.write_changed(root, "_n", Value::Num(12.0));
    assert_eq!(ui.tile(root).get("_n"), Some(&Value::Num(20.0)));
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(external));
    ui.write_changed(root, "_n", Value::Num(13.0));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(external));
    assert_eq!(ui.tile(root).get("_n"), Some(&Value::Num(13.0)));
}

#[test]
fn trait_write_serial_xml_include_and_template_replacement() {
    let (mut ui, root) = serial_tree();
    let src = serial_files();
    let first = ui.trait_write_serial(root, "_n").unwrap();
    assert!(ui.inject_text(&src, "SerialMenu", "<_n>12</_n>"));
    let direct = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(first, direct);
    assert!(ui.inject_text(&src, "SerialMenu", "<include src=\"serial_value.xml\"/>"));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(direct));
    let made = ui.add_from_template(&src, "SerialMenu", "row", Some("Instance")).unwrap();
    let from_template = ui.trait_write_serial(made, "_n").unwrap();
    assert!(ui.inject_file(&src, "SerialMenu/Instance", "menus\\prefabs\\serial_value.xml"));
    assert_ne!(ui.trait_write_serial(made, "_n"), Some(from_template));
    assert_eq!(ui.tile(made).get("_n"), Some(&Value::Num(12.0)));
}

#[test]
fn trait_write_serial_expression_replacement_never_reuses_plain_token() {
    let (mut ui, root) = serial_tree();
    let src = serial_files();
    let plain = ui.trait_write_serial(root, "_n").unwrap();
    assert!(ui.inject_text(&src, "SerialMenu", "<_n><copy>12</copy></_n>"));
    let expression = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(plain, expression);
    let k = ui.tile(root).trait_index("_n").unwrap();
    assert!(ui.tile(root).traits[k].expr.is_some());
    ui.update();
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(expression));
    assert_eq!(ui.tile(root).get("_n"), Some(&Value::Num(12.0)));
    assert!(ui.inject_text(&src, "SerialMenu", "<_n>12</_n>"));
    let restored_plain = ui.trait_write_serial(root, "_n").unwrap();
    assert_ne!(restored_plain, plain);
    assert_ne!(restored_plain, expression);
    assert!(ui.tile(root).traits[ui.tile(root).trait_index("_n").unwrap()].expr.is_none());
}

#[test]
fn trait_write_serial_expression_evaluation_and_direct_override_remain_distinct() {
    let (mut ui, root) = serial_tree();
    let computed = ui.trait_write_serial(root, "_computed").unwrap();
    let constant = ui.trait_write_serial(root, "_constant").unwrap();
    let k = ui.tile(root).trait_index("_computed").unwrap();
    let generation = ui.tile(root).traits[k].changed;
    ui.set_trait(root, "_source", Value::Num(6.0));
    let counts = ui.trait_write_counts();
    ui.update();
    assert_eq!(ui.tile(root).get("_computed"), Some(&Value::Num(7.0)));
    assert!(ui.tile(root).traits[k].changed > generation);
    assert_eq!(ui.trait_write_serial(root, "_computed"), Some(computed));
    assert_eq!(ui.trait_write_serial(root, "_constant"), Some(constant));
    assert_eq!(ui.trait_write_counts(), counts, "evaluation is not an external setter");
    ui.set_trait(root, "_computed", Value::Num(99.0));
    let written = ui.trait_write_serial(root, "_computed").unwrap();
    assert_ne!(written, computed);
    assert!(ui.tile(root).traits[k].expr.is_some());
    ui.update();
    assert_eq!(ui.tile(root).get("_computed"), Some(&Value::Num(99.0)));
    ui.set_trait(root, "_source", Value::Num(8.0));
    ui.update();
    assert_eq!(ui.tile(root).get("_computed"), Some(&Value::Num(9.0)));
    assert_eq!(ui.trait_write_serial(root, "_computed"), Some(written));
}

#[test]
fn trait_write_serial_removal_slot_reuse_and_tree_identity() {
    let (mut ui, root) = serial_tree();
    let before = ui.trait_write_serial(root, "_n").unwrap();
    assert!(ui.remove_trait(root, "_source"));
    assert_eq!(ui.trait_write_serial(root, "_n"), Some(before), "index shifts keep the trait's identity");
    assert!(ui.remove_trait(root, "_n"));
    assert_eq!(ui.trait_write_serial(root, "_n"), None);
    ui.set_trait(root, "_n", Value::Num(12.0));
    assert_ne!(ui.trait_write_serial(root, "_n"), Some(before));
    ui.unload_tile(root);
    assert_eq!(ui.trait_write_serial(root, "_n"), None);
    let replacement = ui.load_menu(&serial_files(), "menus\\serial.xml").unwrap();
    assert_eq!(root.index(), replacement.index(), "the test actually reuses the slot");
    assert_ne!(root, replacement);
    let replacement_stamp = ui.trait_write_serial(replacement, "_n").unwrap();
    ui.set_trait(root, "_n", Value::Num(100.0));
    assert!(!ui.set_existing(root, "_n", Value::Num(100.0)));
    assert_eq!(ui.trait_write_serial(root, "_n"), None);
    assert_eq!(ui.trait_write_serial(replacement, "_n"), Some(replacement_stamp));
    let (other, other_root) = serial_tree();
    assert_eq!(root.index(), other_root.index());
    assert_eq!(other.trait_write_serial(root, "_n"), None);
    assert_eq!(ui.trait_write_serial(other_root, "_n"), None);
}
