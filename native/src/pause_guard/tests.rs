use super::*;
use crate::error::Error;
use crate::memory::ReadMemory;

/// The part of the game the guard depends on.
#[derive(Default)]
struct Game {
    paused: bool,
    hour: i32,
    /// The game menu is shown. Opening it pauses; closing it leaves the game paused.
    menu: bool,
    /// Open views; Escape closes the top one.
    views: u32,
    /// A text box holds the keyboard, so Space is text.
    text_focus: bool,
    /// Looks for which the view just closed still reads as open.
    lag: u32,
    stale: u32,
    /// The next Escape is swallowed (it only leaves a search box).
    swallow: bool,
    /// A view that reads as open although Escape cannot close it.
    stuck: bool,
    escapes: u32,
    clock_keys: u32,
}
impl Game {
    fn escape(&mut self) {
        self.escapes += 1;
        if self.menu {
            self.menu = false;
        } else if self.swallow {
            self.swallow = false;
        } else if self.views > 0 {
            self.views -= 1;
            self.stale = self.lag;
        } else {
            self.menu = true;
            self.paused = true;
        }
    }
    fn space(&mut self) {
        if !self.text_focus && !self.menu {
            self.paused = !self.paused;
        }
    }
    /// What a panel read reports: a closed view lags behind for `stale` looks.
    fn look(&mut self) -> (Seen<u32>, bool) {
        let shown = if self.stale > 0 {
            self.stale -= 1;
            self.views + 1
        } else {
            self.views
        };
        let shown = shown + u32::from(self.stuck);
        let seen = if shown == 0 {
            Seen::Closed
        } else {
            Seen::Open(shown)
        };
        (seen, self.menu)
    }
}

#[derive(Clone, Default)]
struct Keys(Arc<Mutex<Game>>);
impl Keys {
    fn game(&self) -> std::sync::MutexGuard<'_, Game> {
        self.0.lock().unwrap()
    }
    fn watch(&self, window: Duration) -> Arc<Watch> {
        let game = self.0.clone();
        Arc::new(Watch::with_window(
            Box::new(move || {
                let game = game.lock().unwrap();
                Some(Clock {
                    paused: game.paused,
                    hour: game.hour,
                })
            }),
            window,
        ))
    }
    fn watched(&self, window: Duration) -> (Watched<'static>, Arc<Watch>) {
        let watch = self.watch(window);
        (Watched::new(Box::new(self.clone()), watch.clone()), watch)
    }
}
impl GameInput for Keys {
    fn capture(&self, _: (u32, u32)) -> Result<Capture> {
        Err(Error::new("unused", "no capture"))
    }
    fn preflight(&self) -> Result<()> {
        Ok(())
    }
    fn confirm_input_idle(&self) -> Result<()> {
        Ok(())
    }
    fn click(&self, _: (u32, u32), x: f64, y: f64) -> Result<Point> {
        Ok(Point { x, y })
    }
    fn click_checked(
        &self,
        _: (u32, u32),
        x: f64,
        y: f64,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        Ok(Point { x, y })
    }
    fn confirm_cursor_at(&self, _: (u32, u32), _: f64, _: f64) -> Result<()> {
        Ok(())
    }
    fn drag(&self, _: (u32, u32), _: (f64, f64), _: (f64, f64)) -> Result<()> {
        Ok(())
    }
    fn drag_checked(
        &self,
        _: (u32, u32),
        _: (f64, f64),
        _: (f64, f64),
        _: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }
    fn shift_click(&self, _: (u32, u32), x: f64, y: f64) -> Result<Point> {
        Ok(Point { x, y })
    }
    fn right_click(&self, _: (u32, u32), x: f64, y: f64) -> Result<Point> {
        Ok(Point { x, y })
    }
    fn right_drag(&self, _: (u32, u32), _: (f64, f64), _: (f64, f64)) -> Result<()> {
        Ok(())
    }
    fn scroll(&self, _: (u32, u32), _: (f64, f64), _: i32) -> Result<()> {
        Ok(())
    }
    fn press_code(&self, code: u16) -> Result<()> {
        let mut game = self.game();
        match code {
            vk::ESCAPE => game.escape(),
            vk::SPACE => game.space(),
            _ => {}
        }
        Ok(())
    }
    fn press_shifted(&self, code: u16) -> Result<()> {
        self.press_code(code)
    }
    fn press_control(&self, _: u16) -> Result<()> {
        Ok(())
    }
    fn press_clock(&self, key: Key) -> Result<()> {
        self.game().clock_keys += 1;
        self.press(key)
    }
    fn press_once(&self, key: Key) -> key_once::Dispatch {
        let _ = self.press(key);
        key_once::Dispatch::Unknown(None)
    }
    fn press_digit_once(&self, _: char) -> key_once::Dispatch {
        key_once::Dispatch::Unknown(None)
    }
    fn guarded<'a>(
        &'a self,
        _: &'a (dyn Process + Send + Sync),
        _: &'a (dyn Fn() -> Result<()> + Send + Sync),
    ) -> Result<Box<dyn GameInput + 'a>> {
        Ok(Box::new(self.clone()))
    }
}

struct NoProcess;
impl ReadMemory for NoProcess {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        Err(Error::new("unused", "no memory"))
    }
}
impl Process for NoProcess {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
}

/// A window no test outlives: every change right after a key is inside it.
const OPEN: Duration = Duration::from_secs(600);
/// No window at all: only the game date can tie a pause to a key.
const SHUT: Duration = Duration::ZERO;

/// Close the bracket the way the runtime does: the pause key through `press_clock`.
fn end(scope: Scope<'_>, keys: &Watched<'_>, menu: bool) -> Verdict {
    scope.end(menu, &mut |_| keys.press_clock(Key::Pause))
}

#[test]
fn stray_escape_that_opens_the_game_menu_is_undone() {
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    keys.press(Key::Escape).unwrap();
    assert!(game.game().menu && game.game().paused);
    // The runtime closes the menu first; the game stays paused behind it.
    keys.press(Key::Escape).unwrap();
    assert!(!game.game().menu && game.game().paused);
    let verdict = end(scope, &keys, true);
    assert_eq!(
        verdict,
        Verdict {
            before: Some(false),
            after: Some(true),
            ours: true,
            restored: Some(true)
        }
    );
    assert!(!game.game().paused);
    assert_eq!(game.game().clock_keys, 1);
}

#[test]
fn menu_opened_and_closed_inside_the_action_needs_no_menu_evidence() {
    // Two Escapes of some tool's own loop: one opens the menu, the next closes it. Nothing
    // shows a menu afterwards, yet the game stands paused by our key.
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    keys.press(Key::Escape).unwrap();
    keys.press(Key::Escape).unwrap();
    let verdict = end(scope, &keys, false);
    assert!(verdict.ours);
    assert_eq!(verdict.restored, Some(true));
    assert!(!game.game().paused);
}

#[test]
fn space_that_misses_its_text_box_is_undone() {
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    for c in "Army 1".chars() {
        let (code, _) = vk::for_char(c).unwrap();
        keys.press_code(code).unwrap();
    }
    assert!(game.game().paused);
    let verdict = end(scope, &keys, false);
    assert!(verdict.ours);
    assert!(!game.game().paused);
}

#[test]
fn space_typed_into_a_text_box_changes_nothing() {
    let game = Keys::default();
    game.game().text_focus = true;
    let (keys, watch) = game.watched(SHUT);
    let scope = watch.begin();
    keys.press_code(vk::SPACE).unwrap();
    let verdict = end(scope, &keys, false);
    assert_eq!(
        verdict,
        Verdict {
            before: Some(false),
            after: Some(false),
            ours: false,
            restored: None
        }
    );
    assert_eq!(game.game().clock_keys, 0);
}

#[test]
fn pause_by_the_player_during_an_action_is_kept() {
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    keys.click((1920, 1080), 10.0, 10.0).unwrap();
    keys.press(Key::Letter('q')).unwrap();
    game.game().paused = true;
    let verdict = end(scope, &keys, false);
    assert!(!verdict.ours);
    assert!(game.game().paused);
    assert_eq!(game.game().clock_keys, 0);
}

#[test]
fn pause_by_the_host_long_after_our_escape_is_kept() {
    // Our Escape closed a view and the game ran on for a day before the host paused.
    let game = Keys::default();
    game.game().views = 1;
    let (keys, watch) = game.watched(SHUT);
    let scope = watch.begin();
    keys.press(Key::Escape).unwrap();
    assert!(!game.game().paused);
    game.game().hour += 24;
    game.game().paused = true;
    let verdict = end(scope, &keys, false);
    assert!(!verdict.ours);
    assert!(game.game().paused);
}

#[test]
fn pause_read_late_is_still_ours_while_the_date_stands_at_the_key() {
    // The action went on without input for seconds after the stray Space; the game date has
    // not moved since the key, so the pause began with it.
    let game = Keys::default();
    let (keys, watch) = game.watched(SHUT);
    let scope = watch.begin();
    game.game().hour = 100;
    keys.press_code(vk::SPACE).unwrap();
    game.game().hour = 101;
    let verdict = end(scope, &keys, false);
    assert!(verdict.ours);
    assert!(!game.game().paused);
}

#[test]
fn deliberate_clock_key_is_never_undone() {
    // Our Escape closes a view; right after it a date stop pauses on purpose.
    let game = Keys::default();
    game.game().views = 1;
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    keys.press(Key::Escape).unwrap();
    keys.press_clock(Key::Pause).unwrap();
    assert!(game.game().paused);
    let verdict = scope.end(false, &mut |_| panic!("a wanted pause must stay"));
    assert!(!verdict.ours);
    assert!(game.game().paused);
}

#[test]
fn stray_space_on_a_paused_game_is_put_back() {
    // Setup before the first unpause, or a host's pause: the pause is not ours to lift.
    let game = Keys::default();
    game.game().paused = true;
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    keys.press_code(vk::SPACE).unwrap();
    assert!(!game.game().paused);
    let verdict = end(scope, &keys, false);
    assert_eq!(
        verdict,
        Verdict {
            before: Some(true),
            after: Some(false),
            ours: true,
            restored: Some(true)
        }
    );
    assert!(game.game().paused);
}

#[test]
fn unpause_outside_the_window_is_never_answered_with_a_pause() {
    // Whoever started the game after our key, pausing it again would be a pause of ours.
    let game = Keys::default();
    game.game().paused = true;
    game.game().text_focus = true;
    let (keys, watch) = game.watched(SHUT);
    let scope = watch.begin();
    keys.press_code(vk::SPACE).unwrap();
    game.game().paused = false;
    let verdict = scope.end(false, &mut |_| panic!("never pause a running game"));
    assert!(!verdict.ours);
    assert!(!game.game().paused);
}

#[test]
fn restore_tries_once_more_after_clearing_what_took_the_key() {
    // A text box still holds the keyboard after the stray pause: the first pause key is text.
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    keys.press_code(vk::SPACE).unwrap();
    assert!(game.game().paused);
    game.game().text_focus = true;
    let mut attempts = Vec::new();
    let verdict = scope.end(false, &mut |attempt| {
        attempts.push(attempt);
        if attempt > 0 {
            game.game().text_focus = false;
        }
        keys.press_clock(Key::Pause)
    });
    assert_eq!(attempts, [0, 1]);
    assert_eq!(verdict.restored, Some(true));
    assert!(!game.game().paused);
}

#[test]
fn fenced_binding_keeps_the_same_watch() {
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    let process = NoProcess;
    let check = || -> Result<()> { Ok(()) };
    let fenced = keys.guarded(&process, &check).unwrap();
    fenced.press(Key::Escape).unwrap();
    fenced.press(Key::Escape).unwrap();
    let verdict = end(scope, &keys, false);
    assert!(verdict.ours);
    assert!(!game.game().paused);
}

#[test]
fn key_once_escape_is_watched_too() {
    let game = Keys::default();
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    let _ = keys.press_once(Key::Escape);
    let _ = keys.press_once(Key::Escape);
    assert!(end(scope, &keys, false).ours);
    assert!(!game.game().paused);
}

/// The loop the army tools used to close panels with: Escape, a fixed sleep, one read.
fn escape_on_a_timer(game: &Keys, presses: usize) {
    for _ in 0..presses {
        if game.game().look().0 == Seen::Closed {
            break;
        }
        game.press(Key::Escape).unwrap();
    }
    if game.game().menu {
        game.press(Key::Escape).unwrap();
    }
}

fn close(game: &Keys, presses: usize, wait: Duration) -> usize {
    escape_close(presses, wait, &mut || Ok(game.game().look()), &mut || {
        game.press(Key::Escape)
    })
    .unwrap()
}

#[test]
fn timer_driven_escape_pauses_the_game_when_a_closed_view_still_reads_open() {
    // The shakedown failure: the panel a focus or research action left open still read as
    // open one read after the Escape that closed it, so a second Escape went to the free map.
    let game = Keys::default();
    game.game().views = 1;
    game.game().lag = 1;
    escape_on_a_timer(&game, 4);
    assert!(game.game().paused, "the model reproduces the self-pause");
    assert_eq!(game.game().escapes, 3);
}

#[test]
fn escape_waits_for_the_view_it_closed_and_never_reaches_the_menu() {
    let game = Keys::default();
    game.game().views = 1;
    game.game().lag = 5;
    assert_eq!(close(&game, 4, Duration::from_secs(30)), 1);
    assert!(!game.game().paused && !game.game().menu);
    assert_eq!(game.game().escapes, 1);
}

#[test]
fn escape_closes_each_open_view_once() {
    let game = Keys::default();
    game.game().views = 2;
    game.game().lag = 3;
    assert_eq!(close(&game, 4, Duration::from_secs(30)), 2);
    assert!(!game.game().paused && !game.game().menu);
}

#[test]
fn escape_is_not_sent_when_nothing_is_open() {
    let game = Keys::default();
    assert_eq!(close(&game, 4, Duration::from_secs(30)), 0);
    assert_eq!(game.game().escapes, 0);
}

#[test]
fn escape_stops_at_the_game_menu_instead_of_toggling_it() {
    // A view that reads as open although Escape cannot close it: the first Escape opens the
    // game menu. One more would close the menu, the next reopen it; stop at the first.
    let game = Keys::default();
    game.game().stuck = true;
    assert_eq!(close(&game, 4, Duration::from_secs(30)), 1);
    assert!(game.game().menu);
    assert_eq!(game.game().escapes, 1);
}

#[test]
fn swallowed_escape_is_followed_by_one_more() {
    // A search box takes the first Escape and nothing else changes.
    let game = Keys::default();
    game.game().views = 1;
    game.game().swallow = true;
    assert_eq!(close(&game, 3, Duration::ZERO), 2);
    assert!(!game.game().paused && !game.game().menu);
}

#[test]
fn guard_covers_the_whole_failure_from_the_shakedown() {
    // Even with the old loop the shared guard leaves the game running: the pause is ours.
    let game = Keys::default();
    game.game().views = 1;
    game.game().lag = 1;
    let (keys, watch) = game.watched(OPEN);
    let scope = watch.begin();
    for _ in 0..4 {
        if game.game().look().0 == Seen::Closed {
            break;
        }
        keys.press(Key::Escape).unwrap();
    }
    let menu = game.game().menu;
    if menu {
        keys.press(Key::Escape).unwrap();
    }
    assert!(game.game().paused);
    let verdict = end(scope, &keys, menu);
    assert!(verdict.ours);
    assert!(!game.game().paused);
}

/// Every Rust source of the shared management lane (macOS and Linux bind the same code).
fn lane_sources() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push((
                    path.display().to_string(),
                    std::fs::read_to_string(&path).unwrap(),
                ));
            }
        }
    }
    let launch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/launch");
    let mut out = vec![(
        "macos.rs".to_string(),
        std::fs::read_to_string(launch.join("macos.rs")).unwrap(),
    )];
    walk(&launch.join("macos"), &mut out);
    out
}

#[test]
fn the_binding_is_watched_and_the_pause_key_is_only_sent_on_purpose() {
    let sources = lane_sources();
    assert!(sources.len() > 60, "the scan found {}", sources.len());
    // One place binds the input, and it wraps it: no tool can send a key past the watch.
    let lane = &sources[0].1;
    let bind = &lane[lane.find("fn bind_with_input").unwrap()..];
    let bind = &bind[..bind.find("\n    }\n").unwrap()];
    assert!(
        bind.contains("pause_guard::Watched::new")
            && bind.contains("pause_guard::Watch::for_process"),
        "bind_with_input no longer wraps the input binding with the pause watch"
    );
    // The pause key is a clock action: it goes through press_clock, where the watch knows it
    // is wanted. Anywhere else it would be undone as a stray toggle, or worse, not be.
    let mut stray = Vec::new();
    for (path, text) in &sources {
        for (number, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let pause_key = code.contains("Key::Pause") || code.contains("vk::SPACE");
            if pause_key && code.contains("press") && !code.contains("press_clock(") {
                stray.push(format!("{path}:{}", number + 1));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "the pause key is pressed outside press_clock: {stray:?}"
    );
}
