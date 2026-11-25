//! Colour themes: "Swarm" (the app's own look, after Swarm II), the palettes iced ships with, and following the
//! desktop's light or dark mode. A theme is six base colours; the rest of the app's colours are mixed from them,
//! with the contrast of text and accents checked against the background.

use std::cell::Cell;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, Prefs, Theme, ThemeChoice};

/// The choice that follows the desktop: Swarm in dark mode, Light in light mode.
pub const SYSTEM: &str = "Follow system";
pub const DEFAULT: &str = "Swarm";

/// A theme's base colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base {
    pub name: &'static str,
    background: u32,
    text: u32,
    primary: u32,
    success: u32,
    warning: u32,
    danger: u32,
}

const fn base(name: &'static str, background: u32, text: u32, primary: u32, success: u32, warning: u32, danger: u32) -> Base {
    Base { name, background, text, primary, success, warning, danger }
}

/// Swarm first, then iced 0.14's built-in palettes (`iced_core::theme::palette`), in its order.
pub const THEMES: [Base; 23] = [
    base("Swarm", 0x111419, 0xe9ebee, 0x7b45f5, 0x35c47f, 0xe3a23f, 0xe5484d),
    base("Light", 0xffffff, 0x000000, 0x5865f2, 0x12664f, 0xb77e33, 0xc3423f),
    base("Dark", 0x2b2d31, 0xe6e6e6, 0x5865f2, 0x12664f, 0xffc14e, 0xc3423f),
    base("Dracula", 0x282a36, 0xf8f8f2, 0xbd93f9, 0x50fa7b, 0xf1fa8c, 0xff5555),
    base("Nord", 0x2e3440, 0xeceff4, 0x8fbcbb, 0xa3be8c, 0xebcb8b, 0xbf616a),
    base("Solarized Light", 0xfdf6e3, 0x657b83, 0x2aa198, 0x859900, 0xb58900, 0xdc322f),
    base("Solarized Dark", 0x002b36, 0x839496, 0x2aa198, 0x859900, 0xb58900, 0xdc322f),
    base("Gruvbox Light", 0xfbf1c7, 0x282828, 0x458588, 0x98971a, 0xd79921, 0xcc241d),
    base("Gruvbox Dark", 0x282828, 0xfbf1c7, 0x458588, 0x98971a, 0xd79921, 0xcc241d),
    base("Catppuccin Latte", 0xeff1f5, 0x4c4f69, 0x1e66f5, 0x40a02b, 0xdf8e1d, 0xd20f39),
    base("Catppuccin Frappé", 0x303446, 0xc6d0f5, 0x8caaee, 0xa6d189, 0xe5c890, 0xe78284),
    base("Catppuccin Macchiato", 0x24273a, 0xcad3f5, 0x8aadf4, 0xa6da95, 0xeed49f, 0xed8796),
    base("Catppuccin Mocha", 0x1e1e2e, 0xcdd6f4, 0x89b4fa, 0xa6e3a1, 0xf9e2af, 0xf38ba8),
    base("Tokyo Night", 0x1a1b26, 0x9aa5ce, 0x2ac3de, 0x9ece6a, 0xe0af68, 0xf7768e),
    base("Tokyo Night Storm", 0x24283b, 0x9aa5ce, 0x2ac3de, 0x9ece6a, 0xe0af68, 0xf7768e),
    base("Tokyo Night Light", 0xd5d6db, 0x565a6e, 0x166775, 0x485e30, 0x8f5e15, 0x8c4351),
    base("Kanagawa Wave", 0x1f1f28, 0xdcd7ba, 0x7fb4ca, 0x76946a, 0xff9e3b, 0xc34043),
    base("Kanagawa Dragon", 0x181616, 0xc5c9c5, 0x223249, 0x8a9a7b, 0xff9e3b, 0xc4746e),
    base("Kanagawa Lotus", 0xf2ecbc, 0x545464, 0x4d699b, 0x6f894e, 0xe98a00, 0xc84053),
    base("Moonfly", 0x080808, 0xbdbdbd, 0x80a0ff, 0x8cc85f, 0xe3c78a, 0xff5454),
    base("Nightfly", 0x011627, 0xbdc1c6, 0x82aaff, 0xa1cd5e, 0xe3d18a, 0xfc514e),
    base("Oxocarbon", 0x232323, 0xd0d0d0, 0x00b4ff, 0x00c15a, 0xbe95ff, 0xf62d0f),
    base("Ferra", 0x2b292d, 0xfecdb2, 0xd1d1e0, 0xb1b695, 0xf5d76e, 0xe06b75),
];

/// Every colour of the `Theme` global (ui/theme.slint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    pub dark: bool,
    pub background: u32,
    pub rail: u32,
    pub rail_hover: u32,
    pub rail_active: u32,
    pub line: u32,
    pub card_top: u32,
    pub card_bottom: u32,
    pub inset: u32,
    pub field: u32,
    pub hover: u32,
    pub popup: u32,
    pub segment: u32,
    pub outline: u32,
    pub text: u32,
    pub muted: u32,
    pub dim: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub fill_start: u32,
    pub fill_end: u32,
    pub highlight: u32,
    pub track: u32,
    pub knob: u32,
    pub knob_border: u32,
    pub toggle_off: u32,
    pub selected: u32,
    pub selected_soft: u32,
    pub good: u32,
    pub good_soft: u32,
    pub warn: u32,
    pub warn_soft: u32,
    pub warn_border: u32,
    pub bad: u32,
    pub bad_soft: u32,
    pub bad_border: u32,
}

/// Swarm's colours as the app was drawn before themes: the defaults in ui/theme.slint.
const SWARM: Colors = Colors {
    dark: true,
    background: 0x111419,
    rail: 0x0e1014,
    rail_hover: 0x15181e,
    rail_active: 0x181c23,
    line: 0x232831,
    card_top: 0x1d222b,
    card_bottom: 0x191d25,
    inset: 0x1f242c,
    field: 0x252b34,
    hover: 0x262d37,
    popup: 0x20262f,
    segment: 0x313844,
    outline: 0x4a515c,
    text: 0xe9ebee,
    muted: 0x8d949e,
    dim: 0x5b626c,
    accent: 0x7b45f5,
    on_accent: 0xffffff,
    fill_start: 0x6b3ef0,
    fill_end: 0xe2459c,
    highlight: 0x9fc3ff,
    track: 0x3a3f48,
    knob: 0xffffff,
    knob_border: 0xffffff,
    toggle_off: 0x7d838c,
    selected: 0x3dd6b8,
    selected_soft: 0x173a35,
    good: 0x35c47f,
    good_soft: 0x16302a,
    warn: 0xe3a23f,
    warn_soft: 0x2a2416,
    warn_border: 0x4a3c1c,
    bad: 0xe5484d,
    bad_soft: 0x3a1d22,
    bad_border: 0x6b2a33,
};

fn channels(c: u32) -> [f64; 3] {
    [f64::from((c >> 16) & 0xff), f64::from((c >> 8) & 0xff), f64::from(c & 0xff)]
}

/// `a` moved `t` (0 to 1) of the way to `b`.
fn mix(a: u32, b: u32, t: f64) -> u32 {
    let (a, b) = (channels(a), channels(b));
    let c = |i: usize| ((a[i] + (b[i] - a[i]) * t).round().clamp(0.0, 255.0)) as u32;
    c(0) << 16 | c(1) << 8 | c(2)
}

/// Relative luminance (WCAG 2).
fn luminance(c: u32) -> f64 {
    let lin = |v: f64| {
        let v = v / 255.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let [r, g, b] = channels(c);
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// The contrast ratio of two colours (WCAG 2), from 1 to 21.
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `c`, moved towards white or black (whichever is further from `against`) until it has at least `min` contrast
/// with `against`.
fn readable(c: u32, against: u32, min: f64) -> u32 {
    let towards = if luminance(against) < 0.3 { 0xffffff } else { 0x000000 };
    (0..=20).map(|i| mix(c, towards, f64::from(i) * 0.05)).find(|&m| contrast(m, against) >= min).unwrap_or(towards)
}

impl Colors {
    /// The colours of a theme, mixed from its base colours.
    pub fn of(b: &Base) -> Colors {
        if b.name == DEFAULT {
            return SWARM;
        }
        let dark = luminance(b.background) < 0.3;
        // dark themes draw cards a little lighter than the window; light ones lighter still, on a tinted window
        let (background, card_top, card_bottom) = if dark {
            (b.background, mix(b.background, b.text, 0.06), mix(b.background, b.text, 0.04))
        } else {
            (mix(b.background, b.text, 0.06), mix(b.background, 0xffffff, 0.5), b.background)
        };
        let card = card_bottom;
        let text = readable(b.text, card, 5.0);
        let rail = if dark { mix(b.background, 0x000000, 0.25) } else { mix(background, text, 0.04) };
        let accent = readable(b.primary, card, 3.0);
        let good = readable(b.success, card, 3.0);
        let warn = readable(b.warning, card, 3.0);
        let bad = readable(b.danger, card, 3.0);
        let on_accent = if contrast(0xffffff, accent) >= contrast(0x111111, accent) { 0xffffff } else { 0x111111 };
        Colors {
            dark,
            background,
            rail,
            rail_hover: mix(rail, text, 0.04),
            rail_active: mix(rail, text, 0.08),
            line: mix(card, text, 0.12),
            card_top,
            card_bottom,
            inset: mix(card, text, if dark { 0.02 } else { 0.04 }),
            field: mix(card, text, 0.07),
            hover: mix(card, text, 0.09),
            popup: mix(card, text, 0.05),
            segment: mix(card, text, 0.15),
            outline: mix(card, text, 0.28),
            text,
            muted: readable(mix(text, card, 0.4), card, 4.5),
            dim: readable(mix(text, card, 0.62), card, 2.2),
            accent,
            on_accent,
            fill_start: accent,
            fill_end: readable(mix(b.primary, b.danger, 0.5), card, 2.5),
            highlight: readable(mix(accent, text, 0.45), card, 4.5),
            track: mix(card, text, if dark { 0.18 } else { 0.28 }),
            knob: 0xffffff,
            knob_border: if dark { 0xffffff } else { mix(card, text, 0.4) },
            toggle_off: mix(text, card, 0.45),
            selected: good,
            selected_soft: mix(card, good, 0.16),
            good,
            good_soft: mix(card, good, 0.14),
            warn,
            warn_soft: mix(card, warn, 0.14),
            warn_border: mix(card, warn, 0.4),
            bad,
            bad_soft: mix(card, bad, 0.15),
            bad_border: mix(card, bad, 0.35),
        }
    }
}

/// The theme a choice stands for: following the system picks Swarm or Light; an unknown name is Swarm.
pub fn resolve(choice: &str, system_dark: bool) -> &'static Base {
    let name = match choice {
        SYSTEM if system_dark => DEFAULT,
        SYSTEM => "Light",
        name => name,
    };
    THEMES.iter().find(|b| b.name == name).unwrap_or(&THEMES[0])
}

fn color(c: u32) -> slint::Color {
    slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Set every colour of the `Theme` global.
fn set(t: &Theme<'_>, c: &Colors) {
    t.set_dark(c.dark);
    t.set_background(color(c.background));
    t.set_rail(color(c.rail));
    t.set_rail_hover(color(c.rail_hover));
    t.set_rail_active(color(c.rail_active));
    t.set_line(color(c.line));
    t.set_card_top(color(c.card_top));
    t.set_card_bottom(color(c.card_bottom));
    t.set_inset(color(c.inset));
    t.set_field(color(c.field));
    t.set_hover(color(c.hover));
    t.set_popup(color(c.popup));
    t.set_segment(color(c.segment));
    t.set_outline(color(c.outline));
    t.set_text(color(c.text));
    t.set_muted(color(c.muted));
    t.set_dim(color(c.dim));
    t.set_accent(color(c.accent));
    t.set_on_accent(color(c.on_accent));
    t.set_fill_start(color(c.fill_start));
    t.set_fill_end(color(c.fill_end));
    t.set_highlight(color(c.highlight));
    t.set_track(color(c.track));
    t.set_knob(color(c.knob));
    t.set_knob_border(color(c.knob_border));
    t.set_toggle_off(color(c.toggle_off));
    t.set_selected(color(c.selected));
    t.set_selected_soft(color(c.selected_soft));
    t.set_good(color(c.good));
    t.set_good_soft(color(c.good_soft));
    t.set_warn(color(c.warn));
    t.set_warn_soft(color(c.warn_soft));
    t.set_warn_border(color(c.warn_border));
    t.set_bad(color(c.bad));
    t.set_bad_soft(color(c.bad_soft));
    t.set_bad_border(color(c.bad_border));
}

thread_local! {
    // whether the desktop is in dark mode, as last heard; only used on the UI thread
    static SYSTEM_DARK: Cell<bool> = const { Cell::new(true) };
}

/// Show the theme chosen in the window's `Prefs`.
pub fn apply(ui: &AppWindow) {
    let base = resolve(&ui.global::<Prefs>().get_theme(), SYSTEM_DARK.get());
    set(&ui.global::<Theme>(), &Colors::of(base));
}

/// The cards of the theme picker: following the system, then every theme, each in its own colours.
pub fn choices() -> ModelRc<ThemeChoice> {
    let card = |name: &str, label: &str, c: Colors| ThemeChoice {
        name: name.into(),
        label: label.into(),
        background: color(c.background),
        card: color(c.card_top),
        text: color(c.text),
        muted: color(c.muted),
        accent: color(c.accent),
        fill_end: color(c.fill_end),
        good: color(c.good),
    };
    let system = card(SYSTEM, "Swarm or Light", Colors::of(&THEMES[0]));
    let themes = THEMES.iter().map(|b| card(b.name, if luminance(b.background) < 0.3 { "dark" } else { "light" }, Colors::of(b)));
    ModelRc::new(VecModel::from(std::iter::once(system).chain(themes).collect::<Vec<_>>()))
}

/// Whether `name` is a choice the picker offers.
pub fn known(name: &str) -> bool {
    choices().iter().any(|c| c.name == name)
}

/// Follow the desktop's light or dark mode (the settings portal's `color-scheme`) and show it in the window
/// while it follows the system.
pub fn watch_system(ui: slint::Weak<AppWindow>) {
    let report = move |dark: bool| {
        let ui = ui.clone();
        let _ = slint::invoke_from_event_loop(move || {
            SYSTEM_DARK.set(dark);
            if let Some(ui) = ui.upgrade() {
                apply(&ui);
            }
        });
    };
    let spawned = std::thread::Builder::new().name("system-theme".into()).spawn(move || {
        if let Err(e) = follow_portal(report) {
            eprintln!("stealth-gui: cannot follow the desktop's light or dark mode: {e}");
        }
    });
    if let Err(e) = spawned {
        eprintln!("stealth-gui: {e}");
    }
}

/// The portal's `color-scheme`: 1 is dark, 2 light, 0 no preference (taken as dark, Swarm's look).
fn is_dark(value: &zbus::zvariant::Value<'_>) -> Option<bool> {
    match value {
        zbus::zvariant::Value::Value(inner) => is_dark(inner),
        zbus::zvariant::Value::U32(n) => Some(*n != 2),
        _ => None,
    }
}

fn follow_portal(report: impl Fn(bool)) -> zbus::Result<()> {
    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";
    let conn = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )?;
    // listen first, then read, so no change falls in between
    let changes = proxy.receive_signal("SettingChanged")?;
    let now: zbus::Result<zbus::zvariant::OwnedValue> = proxy.call("ReadOne", &(NAMESPACE, KEY));
    let now = match now {
        Ok(v) => Ok(v),
        // portals before version 2 only have Read, which wraps the value once more
        Err(_) => proxy.call::<_, _, zbus::zvariant::OwnedValue>("Read", &(NAMESPACE, KEY)),
    };
    if let Some(dark) = now.ok().as_deref().and_then(is_dark) {
        report(dark);
    }
    for msg in changes {
        let Ok((namespace, key, value)) =
            msg.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
        else {
            continue;
        };
        if namespace == NAMESPACE
            && key == KEY
            && let Some(dark) = is_dark(&value)
        {
            report(dark);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_is_readable() {
        for b in &THEMES {
            let c = Colors::of(b);
            for (what, fg, min) in [
                ("text", c.text, 4.5),
                ("muted", c.muted, 4.0),
                ("accent", c.accent, 3.0),
                ("good", c.good, 3.0),
                ("warn", c.warn, 3.0),
                ("bad", c.bad, 3.0),
            ] {
                let ratio = contrast(fg, c.card_bottom);
                assert!(ratio >= min, "{}: {what} {fg:06x} on {:06x} is {ratio:.2}", b.name, c.card_bottom);
            }
            assert!(contrast(c.on_accent, c.accent) >= 3.0, "{}: button text", b.name);
        }
    }

    #[test]
    fn swarm_keeps_the_old_look_and_choices_resolve() {
        assert_eq!(Colors::of(&THEMES[0]), SWARM);
        assert_eq!(resolve(SYSTEM, true).name, "Swarm");
        assert_eq!(resolve(SYSTEM, false).name, "Light");
        assert_eq!(resolve("Nord", true).name, "Nord");
        assert_eq!(resolve("no such theme", false).name, "Swarm");
        assert!(!Colors::of(resolve("Solarized Light", true)).dark);
        assert!(Colors::of(resolve("Gruvbox Dark", true)).dark);
    }

    #[test]
    fn mixing() {
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
        assert!((contrast(0x000000, 0xffffff) - 21.0).abs() < 0.01);
        assert!(contrast(readable(0x223249, 0x181616, 3.0), 0x181616) >= 3.0);
    }
}
