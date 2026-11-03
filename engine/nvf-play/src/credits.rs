//! The end credits: ExitGame "plays credits and returns the game to the main menu"
//! [documented: GECK wiki ExitGame]. The credits are the game's own `Data\Credits.txt`,
//! laid out by the commands its header comments describe [documented: the file's
//! header]:
//!
//! - `<X:n>`, `<Y:n>`: a position, absolute for a plain number, an offset for `+n` or
//!   `-n` [documented: "n = 128 (absolute position), +10 or -10 (offset positions)"];
//!   after each line of text the font's height is added to y and x goes back to the last
//!   `<X:n>` [documented]. A line's text is placed where y stands after all of the
//!   line's commands [hypothesis: the file ends a heading with `<Y:-15>` and a name with
//!   `<Y:-6>`; applied to the next line instead, each heading overlapped the name under
//!   it on screen].
//!   A blank line adds a line's height too; a line of commands alone adds nothing
//!   [hypothesis].
//! - `<J:L|R|C>` justification, `<F:n>` the font (the INI's `sFontFile_n`), `<C:r,g,b,a>`
//!   the colour [documented].
//! - `<I:wwww,hhhh,aaa,file.dds>` a picture `wwww` wide and `hhhh` high from
//!   `Textures\Interface\Credits`, its height added to y [documented].
//! - Lines starting with `*` are comments [documented].
//!
//! Positions are the credits rectangle's: centred on the screen (`credits_menu.xml`,
//! `credits_rect` at x 640 of the menus' 1280) over the menu's dark blue (8, 8, 20)
//! [documented: menus\options\credits_menu.xml]. Behind the text the game's loading
//! screens take turns, every LSCR picture of the load order in a shuffled order, each
//! dimmed to [`SLIDE_BRIGHTNESS`], panning slowly across the screen while it is up and
//! fading through black to the next as behind the main menu (`crate::main_menu::Phase`)
//! [design: owner 2026-10-25, "credits should have a scrolling background of loading
//! screens"]. The text rolls up from below the screen
//! at [`SPEED`] interface units a second [design: the original's speed is not
//! documented], and only the lines on screen are drawn. Esc, Enter, Space or a click ends
//! it early [design]. The game is held under it (`crate::pause::Paused`); at its end the
//! main menu comes (`crate::pause::MainMenuRequest`). No music is played with it [not
//! built: the original's credits music is not documented].
//!
//! Log: `credits: rolling <n> lines and <m> pictures (<seconds> s)`, `credits: ended
//! (<how>)`.

use crate::interface::{at, face, plain_font, set_line, texture, Face};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;
use std::collections::HashMap;

/// How fast the credits roll, interface units (the screen is 960 high) a second [design].
pub const SPEED: f32 = 60.0;

/// The menu's background colour [documented: credits_menu.xml].
const BACKGROUND: [u8; 3] = [8, 8, 20];

/// The loading screens behind the credits are dimmed to this, so the text reads over
/// them [design].
pub const SLIDE_BRIGHTNESS: f32 = 0.35;
/// Each picture is drawn this much wider than the screen and pans across the extra
/// while it is up [design].
pub const PAN: f32 = 0.15;

/// One thing the credits show, placed in the credits rectangle (x from its centre, y down
/// from the credits' top).
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Text { x: f32, y: f32, justify: char, font: u8, color: [u8; 4], text: String },
    Picture { x: f32, y: f32, w: f32, h: f32, alpha: u8, path: String },
}

impl Item {
    fn top(&self) -> f32 {
        match self {
            Item::Text { y, .. } | Item::Picture { y, .. } => *y,
        }
    }
}

/// A position command's value: an offset from `now` for a signed number, else absolute.
fn position(v: &str, now: f32) -> f32 {
    let v = v.trim();
    let n: f32 = v.parse().unwrap_or(0.0);
    if v.starts_with('+') || v.starts_with('-') {
        now + n
    } else {
        n
    }
}

/// Lay out a credits file (module notes): the items, and the height of the whole.
/// `height(font)` is the height of a line in font `font`.
pub fn parse(src: &str, height: impl Fn(u8) -> f32) -> (Vec<Item>, f32) {
    let (mut x_base, mut y) = (0.0f32, 0.0f32);
    let (mut justify, mut font, mut color) = ('L', 7u8, [255u8, 255, 255, 255]);
    let mut out = Vec::new();
    for raw in src.lines() {
        if raw.starts_with('*') {
            continue;
        }
        let mut text = String::new();
        let mut commands = false;
        let mut rest = raw.trim_end_matches(['\r', '\n']);
        while !rest.is_empty() {
            let cmd = rest.strip_prefix('<').and_then(|r| {
                let end = r.find('>')?;
                let body = &r[..end];
                let (k, v) = body.split_once(':')?;
                (k.len() == 1).then_some((k.chars().next()?, v.to_string(), &r[end + 1..]))
            });
            match cmd {
                Some((k, v, after)) => {
                    commands = true;
                    match k {
                        'X' => x_base = position(&v, x_base),
                        'Y' => y = position(&v, y),
                        'J' => justify = v.trim().chars().next().unwrap_or('L').to_ascii_uppercase(),
                        'F' => font = v.trim().parse().unwrap_or(font),
                        'C' => {
                            let p: Vec<u8> = v.split(',').filter_map(|c| c.trim().parse().ok()).collect();
                            if p.len() == 4 {
                                color = [p[0], p[1], p[2], p[3]];
                            }
                        }
                        'I' => {
                            let p: Vec<&str> = v.split(',').map(str::trim).collect();
                            if let [w, h, a, file] = p[..] {
                                let (w, h) = (w.parse().unwrap_or(0.0), h.parse().unwrap_or(0.0));
                                out.push(Item::Picture { x: x_base, y, w, h, alpha: a.parse().unwrap_or(255), path: format!("textures\\interface\\credits\\{}", file.to_ascii_lowercase()) });
                                y += h;
                            }
                        }
                        _ => {}
                    }
                    rest = after;
                }
                None => {
                    let mut chars = rest.chars();
                    let c = chars.next().unwrap_or(' ');
                    text.push(c);
                    rest = chars.as_str();
                }
            }
        }
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(Item::Text { x: x_base, y, justify, font, color, text });
            y += height(font);
        } else if !commands {
            y += height(font);
        }
    }
    (out, y)
}

/// The credits rolling: their items, how far they have rolled, the lines drawn now.
#[derive(Resource, Default)]
pub struct Credits {
    items: Vec<Item>,
    total: f32,
    scroll: f32,
    open: bool,
    faces: HashMap<u8, Face>,
    drawn: HashMap<usize, Entity>,
    root: Option<Entity>,
    /// The loading screens' pictures behind the text, the one up, its phase and how long
    /// it has been up; the picture's entity.
    slides: Vec<String>,
    slide: usize,
    phase: Option<crate::main_menu::Phase>,
    slide_age: f32,
    picture: Option<Entity>,
}

impl Credits {
    pub fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Component)]
struct CreditsRoot;

/// The loading screen behind the credits' text.
#[derive(Component)]
struct CreditsPicture;

/// The loading screen's picture, its placement and whether it shows.
type Pictures<'w, 's> = Query<'w, 's, (&'static mut ImageNode, &'static mut Node, &'static mut Visibility), With<CreditsPicture>>;

/// The window's size in pixels.
fn screen_px(windows: &Query<&Window, With<PrimaryWindow>>) -> Vec2 {
    windows.single().map_or(Vec2::new(1280.0, 960.0), |w| Vec2::new(w.width(), w.height()))
}

/// The loading screens behind the credits: one after another with a fade through black,
/// each panning across [`PAN`] of its width while it is up.
#[allow(clippy::too_many_arguments)]
fn slideshow(
    c: &mut Credits,
    dt: f32,
    window: Vec2,
    data: Option<&GameData>,
    textures: Option<ResMut<nvf_assets::TextureCache>>,
    images: &mut Assets<Image>,
    formats: Option<&CompressedImageFormatSupport>,
    pictures: &mut Pictures,
) {
    let (Some(phase), Some(pic)) = (c.phase, c.picture) else { return };
    let Ok((mut img, mut node, mut vis)) = pictures.get_mut(pic) else { return };
    if c.slides.is_empty() {
        return;
    }
    let (phase, next) = phase.step(dt.min(0.1));
    c.phase = Some(phase);
    c.slide_age += dt;
    if next {
        c.slide = (c.slide + 1) % c.slides.len();
        c.slide_age = 0.0;
    }
    if next || *vis == Visibility::Hidden {
        let want = c.slides[c.slide].clone();
        if let (Some(d), Some(mut cache)) = (data, textures) {
            match cache.get(&want, true, d, images, formats) {
                Some(h) => {
                    img.image = h;
                    *vis = Visibility::Inherited;
                }
                None => {
                    c.slide = (c.slide + 1) % c.slides.len();
                    return;
                }
            }
        }
    }
    let b = SLIDE_BRIGHTNESS;
    img.color = Color::srgba(b, b, b, phase.alpha());
    // Cover the window and a pan's width more; move across it over the slide's turn.
    if let Some(i) = images.get(&img.image) {
        let (size, offset) = crate::loading::cover(i.size().as_vec2(), window * (1.0 + PAN));
        let turn = crate::main_menu::HOLD_SECS + 2.0 * crate::main_menu::FADE_SECS;
        let along = (c.slide_age / turn).clamp(0.0, 1.0);
        node.width = Val::Px(size.x);
        node.height = Val::Px(size.y);
        node.left = Val::Px(offset.x - window.x * PAN * along);
        node.top = Val::Px(offset.y - window.y * PAN * 0.5);
    }
}

/// What [`start`] needs, as system parameters.
pub type Start<'w> = (ResMut<'w, Credits>, Option<Res<'w, GameData>>, ResMut<'w, Assets<Image>>, Option<Res<'w, CompressedImageFormatSupport>>);

/// Ask for the credits to roll (ExitGame). Whether they could be read.
pub fn start(credits: &mut Credits, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> bool {
    // A top-level file of Data, outside the archives' namespace.
    let Some(src) = data.and_then(|d| std::fs::read(d.vfs.data_dir().join("Credits.txt")).ok()) else { return false };
    // 8-bit text in this run's code page (fnv_util::text, #1439).
    let src = fnv_util::text::decode(&src);
    for n in 1..=9u8 {
        credits.faces.entry(n).or_insert_with(|| face(crate::interface::FONT_FILES[n as usize - 1], plain_font(24.0, 12.0), data, images, bc));
    }
    let faces = &credits.faces;
    let (items, total) = parse(&src, |f| faces.get(&f).map_or(24.0, |x| x.font.size));
    let pictures = items.iter().filter(|i| matches!(i, Item::Picture { .. })).count();
    info!("credits: rolling {} lines and {pictures} pictures ({:.0} s)", items.len() - pictures, (total + 960.0) / SPEED);
    credits.items = items;
    credits.total = total;
    credits.scroll = 0.0;
    credits.open = true;
    true
}

pub struct CreditsPlugin;

impl Plugin for CreditsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Credits>().add_systems(Update, roll);
    }
}

#[allow(clippy::too_many_arguments)]
fn roll(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut credits: ResMut<Credits>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut paused: ResMut<crate::pause::Paused>,
    mut to_main: MessageWriter<crate::pause::MainMenuRequest>,
    (mut lines, mut pictures): (Query<&mut Node, Without<CreditsPicture>>, Pictures),
    (play, textures): (Option<Res<crate::PlayWorld>>, Option<ResMut<nvf_assets::TextureCache>>),
) {
    if !credits.open {
        return;
    }
    paused.0 = true;
    let screen = windows.single().map_or(Vec2::new(1280.0, 960.0), |w| Vec2::new(960.0 * w.width() / w.height().max(1.0), 960.0));
    credits.scroll += SPEED * time.delta_secs();
    let skipped = keys.any_just_pressed([KeyCode::Escape, KeyCode::Enter, KeyCode::Space]) || mouse.just_pressed(MouseButton::Left);
    if skipped || credits.scroll > credits.total + screen.y {
        info!("credits: ended ({})", if skipped { "skipped" } else { "rolled to the end" });
        if let Some(r) = credits.root.take() {
            commands.entity(r).despawn();
        }
        credits.drawn.clear();
        credits.picture = None;
        credits.phase = None;
        credits.open = false;
        to_main.write(crate::pause::MainMenuRequest);
        return;
    }
    let root = match credits.root {
        Some(r) => r,
        None => {
            let [r, g, b] = BACKGROUND;
            let r = commands
                .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgb_u8(r, g, b)), GlobalZIndex(130), CreditsRoot))
                .id();
            credits.root = Some(r);
            // The loading screens behind the text, over the background colour that shows
            // between them.
            let pic = commands.spawn((ImageNode::default(), Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, CreditsPicture, ChildOf(r))).id();
            credits.picture = Some(pic);
            if let Some(play) = play.as_deref() {
                let screens = fnv_world::loadscreen::load_screens(play.0.lo, play.0.schema);
                let mut slides: Vec<String> = screens.iter().filter_map(|s| s.texture.clone()).collect();
                slides.sort();
                slides.dedup();
                crate::main_menu::shuffle(&mut slides);
                info!("credits: {} loading screens behind them", slides.len());
                credits.slides = slides;
            }
            credits.slide = 0;
            credits.slide_age = 0.0;
            credits.phase = Some(crate::main_menu::Phase::In(0.0));
            r
        }
    };
    slideshow(&mut credits, time.delta_secs(), screen_px(&windows), data.as_deref(), textures, &mut images, formats.as_deref(), &mut pictures);
    // The items on screen: drawn as they come in, taken away as they leave.
    let top_of = |item: &Item, scroll: f32| screen.y - scroll + item.top();
    let scroll = credits.scroll;
    let bc = crate::interface::bc_supported(formats.as_deref());
    let c = &mut *credits;
    let visible: Vec<usize> = (0..c.items.len()).filter(|&i| (-120.0..screen.y + 20.0).contains(&top_of(&c.items[i], scroll))).collect();
    c.drawn.retain(|i, e| {
        let keep = visible.contains(i);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    let middle = screen.x / 2.0;
    for i in visible {
        let top = top_of(&c.items[i], scroll);
        if let Some(e) = c.drawn.get(&i) {
            if let Ok(mut n) = lines.get_mut(*e) {
                n.top = crate::interface::u(top);
            }
            continue;
        }
        let line = commands.spawn((at(0.0, top, screen.x, 1.0), ChildOf(root))).id();
        match &c.items[i] {
            Item::Text { x, justify, font, color, text, .. } => {
                let Some(f) = c.faces.get(font) else { continue };
                let l = f.font.lines(text, None).into_iter().next().unwrap_or_default();
                let left = match justify {
                    'R' => middle + x - l.width,
                    'C' => middle + x - l.width / 2.0,
                    _ => middle + x,
                };
                let colour = Color::srgba_u8(color[0], color[1], color[2], color[3]);
                commands.entity(line).with_children(|p| set_line(p, f, &l, left, f.font.size, colour));
            }
            Item::Picture { x, w, h, alpha, path, .. } => {
                if let Some((img, _)) = data.as_deref().and_then(|d| texture(path, d, &mut images, bc)) {
                    commands.entity(line).with_children(|p| {
                        p.spawn((ImageNode { image: img, color: Color::srgba(1.0, 1.0, 1.0, *alpha as f32 / 255.0), ..default() }, at(middle + x, 0.0, *w, *h)));
                    });
                }
            }
        }
        c.drawn.insert(i, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_commands_place_its_lines() {
        let src = "* a comment\n<X:-500><Y:0><J:L><F:7>\n\n<I:0256,0064,255,Logo.dds>\n<C:255,207,119,255>PUBLISHED BY<Y:-6>\n<C:1,2,3,255>Someone<Y:-6>\n<X:+100><J:C>Centred\n";
        let (items, total) = parse(src, |_| 20.0);
        // The blank line adds a line's height; the commands-only line nothing.
        assert_eq!(items[0], Item::Picture { x: -500.0, y: 20.0, w: 256.0, h: 64.0, alpha: 255, path: "textures\\interface\\credits\\logo.dds".into() });
        // A line's <Y:-6> moves that line itself up.
        assert_eq!(items[1], Item::Text { x: -500.0, y: 78.0, justify: 'L', font: 7, color: [255, 207, 119, 255], text: "PUBLISHED BY".into() });
        assert_eq!(items[2], Item::Text { x: -500.0, y: 92.0, justify: 'L', font: 7, color: [1, 2, 3, 255], text: "Someone".into() });
        assert_eq!(items[3], Item::Text { x: -400.0, y: 112.0, justify: 'C', font: 7, color: [1, 2, 3, 255], text: "Centred".into() });
        assert_eq!(total, 132.0);
    }
}
