//! Expectations taken from UI SDK's own test suite (`packages/core/test`), restated
//! against armature. Each test names the upstream file it follows. Units differ in
//! one place: upstream measures velocity in px/ms, armature in units per second.

use armature_core::gesture::{
    DECELERATION, VelocityTracker, dim_amount, expand_amount, project_distance, rubber_band,
};
use armature_core::pagination::{PageItem, page_count, page_range};
use armature_core::typeahead::Typeahead;

fn close(left: f64, right: f64, tolerance: f64) -> bool {
    (left - right).abs() <= tolerance
}

fn row(page: usize, count: usize, siblings: usize, boundaries: usize) -> String {
    let items = page_range(page, count, siblings, boundaries);
    let text: Vec<String> = items
        .iter()
        .map(|item| match item {
            PageItem::Page(page) => page.to_string(),
            PageItem::EllipsisStart | PageItem::EllipsisEnd => "…".to_string(),
        })
        .collect();
    text.join(" ")
}

// pagination.test.ts
#[test]
fn pagination_range() {
    assert!(page_range(1, 0, 1, 1).is_empty());
    assert_eq!(row(1, 1, 1, 1), "1");
    assert_eq!(row(1, 2, 1, 1), "1 2");
    assert_eq!(row(4, 7, 1, 1), "1 2 3 4 5 6 7");
    let every: Vec<String> = (1..=8).map(|page| row(page, 8, 1, 1)).collect();
    let (early, late) = ("1 2 3 4 5 … 8", "1 … 4 5 6 7 8");
    assert_eq!(every, [early, early, early, early, late, late, late, late]);
    assert_eq!(page_range(5, 10, 1, 1)[1], PageItem::EllipsisStart);
    assert_eq!(row(5, 10, 1, 1), "1 … 4 5 6 … 10");
    assert_eq!(row(50, 100, 1, 1), "1 … 49 50 51 … 100");
    for (count, siblings, boundaries) in [
        (10, 1, 1),
        (100, 2, 1),
        (37, 0, 0),
        (50, 3, 2),
        (12, 1, 2),
        (1000, 1, 1),
    ] {
        let lengths: std::collections::HashSet<usize> = (1..=count)
            .map(|page| page_range(page, count, siblings, boundaries).len())
            .collect();
        assert_eq!(
            lengths.len(),
            1,
            "{count} pages, {siblings} siblings, {boundaries} boundaries"
        );
    }
    assert_eq!(row(4, 10, 1, 1), "1 2 3 4 5 … 10");
    assert_eq!(row(7, 10, 1, 1), "1 … 6 7 8 9 10");
    for count in [1, 2, 7, 8, 9, 10, 23, 100] {
        for page in 1..=count {
            let pages: Vec<usize> = page_range(page, count, 1, 1)
                .iter()
                .filter_map(|item| {
                    if let PageItem::Page(page) = item {
                        Some(*page)
                    } else {
                        None
                    }
                })
                .collect();
            assert!(pages.contains(&page));
            assert!(
                pages.windows(2).all(|pair| pair[0] < pair[1]),
                "in order, each once"
            );
        }
    }
    assert_eq!(
        [
            page_count(95, 10),
            page_count(100, 10),
            page_count(101, 10),
            page_count(0, 10),
            page_count(10, 0)
        ],
        [10, 10, 11, 1, 1]
    );
}

// gesture/projection.test.ts and gesture/rubber-band.test.ts
#[test]
fn projection_and_rubber_band() {
    assert_eq!(DECELERATION, 0.998);
    assert!(close(project_distance(1000.0, 0.998), 499.0, 1e-5));
    assert!(close(project_distance(1000.0, 0.99), 99.0, 1e-5));
    assert!(close(rubber_band(100.0, 800.0, 0.55), 51.5, 0.05));
    assert!(close(rubber_band(200.0, 800.0, 0.55), 96.7, 0.05));
    assert!(close(rubber_band(400.0, 800.0, 0.55), 172.5, 0.05));
    assert!(rubber_band(1e9, 800.0, 0.55) < 800.0);
}

// gesture/curves.test.ts
#[test]
fn sheet_curves() {
    let snaps = [160.0, 400.0, 800.0];
    assert_eq!(
        [
            dim_amount(0.0, &snaps, None),
            dim_amount(80.0, &snaps, None),
            dim_amount(600.0, &snaps, None)
        ],
        [0.0, 0.5, 1.0]
    );
    assert_eq!(
        [
            dim_amount(400.0, &snaps, Some(1)),
            dim_amount(600.0, &snaps, Some(1)),
            dim_amount(800.0, &snaps, Some(1))
        ],
        [0.0, 0.5, 1.0]
    );
    assert_eq!(
        [
            expand_amount(400.0, &snaps),
            expand_amount(700.0, &snaps),
            expand_amount(500.0, &[800.0])
        ],
        [0.0, 0.75, 0.625]
    );
}

// gesture/velocity.test.ts
#[test]
fn velocity() {
    let mut rested = VelocityTracker::default();
    for t in (0..=100).step_by(8) {
        rested.push(t as f64, t as f64 * 2.0);
    }
    assert_eq!(
        rested.velocity_at(150.0),
        0.0,
        "a 50ms rest before lifting reads 0"
    );

    let mut repeated = VelocityTracker::default();
    for (time, position) in [(0.0, 0.0), (8.0, 8.0), (8.0, 500.0), (16.0, 16.0)] {
        repeated.push(time, position);
    }
    assert!(
        close(repeated.velocity_at(16.0), 1000.0, 1e-2),
        "samples with a repeated timestamp are dropped: {}",
        repeated.velocity_at(16.0)
    );

    let mut fast = VelocityTracker::default();
    for t in (0..=40).step_by(8) {
        fast.push(t as f64, t as f64 * 20.0);
    }
    assert_eq!(
        fast.velocity_at(40.0),
        8000.0,
        "the estimate is clamped to 8px/ms"
    );
}

fn typed(labels: &[&str], keys: &str, current: Option<usize>) -> Option<String> {
    let items: Vec<(&str, bool)> = labels.iter().map(|label| (*label, true)).collect();
    let mut search = Typeahead::default();
    let mut index = current;
    for (at, key) in keys.chars().enumerate() {
        if let Some(found) = search.push(key, at as u64, &items, index) {
            index = Some(found);
        }
    }
    index.map(|index| labels[index].to_string())
}

// typeahead.test.ts
#[test]
fn typeahead() {
    assert_eq!(
        typed(&["Fichier", "Éditer"], "e", None).as_deref(),
        Some("Éditer")
    );
    assert_eq!(
        typed(&["Файл", "Журнал"], "ж", None).as_deref(),
        Some("Журнал")
    );
    assert_eq!(
        typed(&["Lamp", "Lock", "Note"], "ll", None).as_deref(),
        Some("Lock")
    );
    assert_eq!(
        typed(&["Lamp", "llama", "Lock"], "ll", None).as_deref(),
        Some("llama")
    );
    assert_eq!(
        typed(&["Date", "Delete"], "d", Some(0)).as_deref(),
        Some("Delete")
    );
    assert_eq!(
        typed(&["Share", "Show sidebar"], "sha", Some(0)).as_deref(),
        Some("Share")
    );
    let bars: Vec<(&str, bool)> = ["Bar", "Car", "Bat"]
        .iter()
        .map(|label| (*label, true))
        .collect();
    let mut search = Typeahead::default();
    assert_eq!(search.push('b', 0, &bars, None), Some(0));
    assert_eq!(
        search.push('a', 1, &bars, None),
        Some(0),
        "a longer query starts from the first item"
    );
    assert_eq!(
        typed(&["Lamp", "Lock", "Loop", "Note"], "lll", None).as_deref(),
        Some("Loop")
    );
    assert_eq!(
        Typeahead::default().push(' ', 0, &[("Open file", true)], None),
        None,
        "space only continues a search"
    );
}

use armature_core::date::{Date, days_in_month, month_grid};
use armature_core::tree::{CheckStrategy, Checked, Tree, TreeItem};
use std::collections::HashSet;

fn day(text: &str) -> Date {
    Date::parse(text).unwrap_or_else(|| panic!("{text} is a date"))
}

// date.test.ts
#[test]
fn date_arithmetic() {
    assert!(Date::parse("2026-10-16").is_some() && Date::parse("2028-02-29").is_some());
    for bad in [
        "2026-02-29",
        "2026-13-01",
        "2026-1-16",
        "2026-10-16T00:00",
        "",
    ] {
        assert!(Date::parse(bad).is_none(), "{bad} is not a date");
    }
    assert_eq!(
        Date::new(2026, 10, 16).map(|date| date.format()).as_deref(),
        Some("2026-10-16")
    );
    assert_eq!(
        Date::new(33, 1, 5).map(|date| date.format()).as_deref(),
        Some("0033-01-05")
    );
    assert!(Date::new(2026, 2, 30).is_none());
    assert_eq!(
        [
            days_in_month(2026, 1),
            days_in_month(2026, 4),
            days_in_month(2026, 2),
            days_in_month(2028, 2),
            days_in_month(1900, 2),
            days_in_month(2000, 2)
        ],
        [31, 30, 28, 29, 28, 29]
    );
    for (from, days, to) in [
        ("2026-10-31", 1, "2026-11-01"),
        ("2026-12-31", 1, "2027-01-01"),
        ("2026-03-01", -1, "2026-02-28"),
        ("2026-03-29", -1, "2026-03-28"),
        ("2026-10-16", 365, "2027-10-16"),
    ] {
        assert_eq!(day(from).add_days(days).format(), to);
    }
    for (from, months, to) in [
        ("2026-01-31", 1, "2026-02-28"),
        ("2028-01-31", 1, "2028-02-29"),
        ("2026-03-31", -1, "2026-02-28"),
        ("2026-11-16", 2, "2027-01-16"),
        ("2026-01-16", -13, "2024-12-16"),
    ] {
        assert_eq!(day(from).add_months(months).format(), to);
    }
    assert_eq!(day("2028-02-29").add_years(1).format(), "2029-02-28");
    assert_eq!(day("2026-10-16").add_years(-10).format(), "2016-10-16");
    // Arithmetic stops at the first and last days there are.
    assert_eq!(day("9999-12-31").add_days(1).format(), "9999-12-31");
    assert_eq!(day("0001-01-16").add_months(-1).format(), "0001-01-01");
    assert_eq!(day("9999-06-01").add_years(1).format(), "9999-12-31");
    assert_eq!(day("2026-03-01").days_until(day("2026-04-01")), 31);
    assert_eq!(day("2026-10-20").days_until(day("2026-10-16")), -4);
    assert_eq!(day("2000-01-01").days_until(day("2026-10-16")), 9785);
    for (date, weekday) in [
        ("2026-10-16", 5),
        ("2026-10-18", 0),
        ("1970-01-01", 4),
        ("1969-12-31", 3),
        ("0001-01-01", 1),
    ] {
        assert_eq!(day(date).weekday(), weekday, "{date}");
    }
    assert_eq!(day("2026-10-16").start_of_week(0).format(), "2026-10-11");
    assert_eq!(day("2026-10-16").start_of_week(1).format(), "2026-10-12");
    assert_eq!(day("2026-10-12").start_of_week(1).format(), "2026-10-12");
    assert_eq!(day("2026-10-16").start_of_week(6).format(), "2026-10-10");
    assert_eq!(day("2026-10-16").end_of_week(1).format(), "2026-10-18");
    assert_eq!(day("2026-02-10").end_of_month().format(), "2026-02-28");
    assert_eq!(day("2026-10-16").end_of_month().format(), "2026-10-31");
    assert_eq!(
        day("2026-10-16")
            .clamped(Date::parse("2026-10-20"), None)
            .format(),
        "2026-10-20"
    );
    assert_eq!(
        day("2026-10-16")
            .clamped(None, Date::parse("2026-10-01"))
            .format(),
        "2026-10-01"
    );
}

// date.test.ts, "month grids"
#[test]
fn month_grids() {
    let weeks = month_grid(2026, 10, 0);
    let first: Vec<String> = weeks[0].iter().map(|date| date.format()).collect();
    assert_eq!(
        first,
        [
            "2026-09-27",
            "2026-09-28",
            "2026-09-29",
            "2026-09-30",
            "2026-10-01",
            "2026-10-02",
            "2026-10-03"
        ]
    );
    assert_eq!(month_grid(2026, 10, 1)[0][0].format(), "2026-09-28");
    assert_eq!(month_grid(2026, 11, 1)[0][0].format(), "2026-10-26");
    // Upstream's `fixedWeeks: false`, which is how armature always draws a month.
    assert_eq!(month_grid(2026, 2, 0).len(), 4);
    assert_eq!(month_grid(2026, 10, 0).len(), 5);
    // Upstream's default, six weeks whatever the month.
    let fixed = armature_core::date::month_grid_fixed(2026, 10, 0);
    assert_eq!(fixed.len(), 6);
    assert_eq!(fixed[5][6].format(), "2026-11-07");
}

fn node(key: &str, children: Vec<TreeItem>) -> TreeItem {
    TreeItem {
        key: key.into(),
        label: key.into(),
        children,
    }
}

fn produce() -> Vec<TreeItem> {
    vec![
        node(
            "fruit",
            vec![
                node("apple", vec![]),
                node("berries", vec![node("straw", vec![]), node("blue", vec![])]),
                node("lemon", vec![]),
            ],
        ),
        node("veg", vec![node("carrot", vec![])]),
        node("nuts", vec![]),
    ]
}

fn set(keys: &[&str]) -> HashSet<String> {
    keys.iter().map(|key| key.to_string()).collect()
}

// tree-checked.test.ts
#[test]
fn tree_checking() {
    let items = produce();
    let tree = Tree::new(&items);
    let states = tree.checked_states(&set(&["straw", "blue", "apple"]));
    assert_eq!(states["berries"], Checked::Checked);
    assert_eq!(states["fruit"], Checked::Mixed);
    assert_eq!(states["lemon"], Checked::Unchecked);
    assert_eq!(states["veg"], Checked::Unchecked);
    // A branch whose children have not loaded checks like a leaf.
    assert_eq!(
        tree.checked_states(&set(&["nuts"]))["nuts"],
        Checked::Checked
    );

    let berries = tree.toggle_checked("berries", &HashSet::new());
    assert_eq!(
        tree.checked_value(&berries, CheckStrategy::Leaf),
        ["straw", "blue"]
    );
    assert_eq!(
        tree.checked_value(&berries, CheckStrategy::Branch),
        ["berries", "straw", "blue"]
    );
    assert_eq!(
        tree.checked_value(&berries, CheckStrategy::Trunk),
        ["berries"]
    );
    // Checking the last leaf makes the whole branch the trunk.
    let all = tree.toggle_checked("lemon", &set(&["apple", "straw", "blue"]));
    assert_eq!(tree.checked_value(&all, CheckStrategy::Trunk), ["fruit"]);
    // Pressing a mixed branch checks everything in it, and a checked one unchecks it.
    let mixed = tree.toggle_checked("fruit", &set(&["apple", "carrot"]));
    assert_eq!(
        tree.checked_value(&mixed, CheckStrategy::Leaf),
        ["apple", "straw", "blue", "lemon", "carrot"]
    );
    let cleared = tree.toggle_checked("fruit", &set(&["apple", "straw", "blue", "lemon"]));
    assert!(tree.checked_value(&cleared, CheckStrategy::Leaf).is_empty());
}

use armature_core::number::{NumberRange, decimals_of, wrap};

// number.test.ts
#[test]
fn number_steps_and_scales() {
    assert_eq!(
        [
            decimals_of(1.0),
            decimals_of(0.25),
            decimals_of(1e-7),
            decimals_of(1.5e-7)
        ],
        [0, 2, 7, 8]
    );
    assert_eq!(
        NumberRange::new(0.0, 1.0, 0.1).snap(0.30000000000000004),
        0.3
    );
    assert_eq!(NumberRange::new(1.0, 100.0, 5.0).snap(7.0), 6.0);
    assert_eq!(NumberRange::new(-10.0, 10.0, 0.5).snap(-3.4), -3.5);
    let thirds = NumberRange::new(0.0, 100.0, 3.0);
    assert_eq!((thirds.max_on_grid(), thirds.snap(100.0)), (99.0, 99.0));
    assert_eq!(
        NumberRange::new(f64::NEG_INFINITY, f64::INFINITY, 1.0).snap(12.37),
        12.0
    );
    assert_eq!(
        NumberRange::new(f64::NEG_INFINITY, f64::INFINITY, 0.25).snap(12.37),
        12.25
    );
    let range = NumberRange::new(0.0, 10.0, 1.0);
    assert_eq!(
        [
            range.nudge(1.5, 1.0),
            range.nudge(1.5, -1.0),
            range.nudge(2.0, 1.0),
            range.nudge(10.0, 1.0)
        ],
        [2.0, 1.0, 3.0, 10.0]
    );
    let tenths = NumberRange::new(0.0, 1.0, 0.1);
    let thrice = (0..3).fold(0.0, |value, _| tenths.nudge(value, 1.0));
    assert_eq!(thrice, 0.3);
    assert_eq!(NumberRange::new(0.0, 1.0, 1e-7).nudge(0.0, 1.0), 1e-7);
    let cents = NumberRange::new(f64::NEG_INFINITY, f64::INFINITY, 0.01);
    assert_eq!(cents.snap(98765432101.23), 98765432101.23);
    assert_eq!(cents.nudge(98765432101.23, 1.0), 98765432101.24);
    assert_eq!(
        [
            wrap(370.0, 0.0, 360.0),
            wrap(-10.0, 0.0, 360.0),
            wrap(360.0, 0.0, 360.0),
            wrap(7.0, 0.0, 5.0),
            wrap(3.0, 2.0, 2.0)
        ],
        [10.0, 350.0, 0.0, 2.0, 2.0]
    );
    let plain = NumberRange::new(0.0, 200.0, 0.0);
    assert_eq!(
        (plain.fraction(50.0), plain.at_fraction(0.25)),
        (0.25, 50.0)
    );
    assert_eq!(
        (
            NumberRange::new(0.0, 10.0, 0.0).fraction(-5.0),
            NumberRange::new(0.0, 10.0, 0.0).fraction(15.0)
        ),
        (0.0, 1.0)
    );
    assert_eq!(NumberRange::new(3.0, 3.0, 0.0).fraction(5.0), 0.0);
    assert!((NumberRange::new(2.0, 2000.0, 0.0).log().fraction(200.0) - 2.0 / 3.0).abs() < 1e-9);
    let log = NumberRange::new(20.0, 20000.0, 0.0).log();
    assert!((log.at_fraction(0.5) - (20.0f64 * 20000.0).sqrt()).abs() < 1e-6);
    assert!((log.at_fraction(log.fraction(1234.0)) - 1234.0).abs() < 1e-6);
}

use armature_core::file_input::accepts_file;
use armature_core::gauge::percent;

// files.test.ts and gauge.test.ts
#[test]
fn file_kinds_and_gauges() {
    let rules = |text: &str| -> Vec<String> { text.split(',').map(str::to_string).collect() };
    let tokens = rules("image/*,application/pdf,.csv");
    assert!(accepts_file("a.png", "image/png", &tokens));
    assert!(accepts_file("a.pdf", "application/pdf", &tokens));
    assert!(!accepts_file("a.txt", "text/plain", &tokens));
    assert!(accepts_file("a.txt", "text/plain", &rules("*/*")));
    assert!(accepts_file("a.txt", "text/plain", &[]));
    // Windows reports CSV files as Excel files, and some platforms know no type.
    assert!(accepts_file(
        "data.CSV",
        "application/vnd.ms-excel",
        &rules(".csv")
    ));
    assert!(accepts_file("photo.heic", "", &rules(".heic")));
    assert!(!accepts_file("photo.heic", "", &rules("image/*")));
    assert!(accepts_file(
        "a.csv",
        "Text/CSV; charset=utf-8",
        &rules("text/csv")
    ));
    assert!(accepts_file(
        "a.pdf",
        "application/pdf",
        &rules(" image/*, .PDF ,,")
    ));

    assert_eq!(
        [
            percent(30.0, 0.0, 60.0),
            percent(-5.0, 0.0, 100.0),
            percent(150.0, 0.0, 100.0),
            percent(5.0, 5.0, 5.0)
        ],
        [50.0, 0.0, 100.0, 100.0]
    );
}

use armature_core::fold::fold_text;
use armature_core::text_match::{match_ranges, match_segments};

// match.test.ts
#[test]
fn folding_and_match_ranges() {
    assert_eq!(fold_text("Łódź", None), "lodz");
    assert_eq!(fold_text("Smørrebrød", None), "smorrebrod");
    assert_eq!(fold_text("Crème Brûlée", None), "creme brulee");
    assert_eq!(fold_text("İstanbul", None), "istanbul");
    // A language's own letters are kept in that language.
    assert_eq!(fold_text("Älmhult", Some("sv")), "älmhult");
    assert_eq!(fold_text("Älmhult", Some("en")), "almhult");
    assert_eq!(fold_text("ISPARTA", Some("tr")), "ısparta");
    assert_eq!(fold_text("ISPARTA", Some("en")), "isparta");

    // Upstream counts characters. armature's ranges are byte offsets into the text.
    let text = "Crème brûlée";
    let ranges = match_ranges("brule", text).expect("accents are no difference");
    assert_eq!(ranges.len(), 1);
    assert_eq!(&text[ranges[0].clone()], "brûlé");
    let segments: Vec<(&str, bool)> = match_segments(text, &ranges)
        .iter()
        .map(|segment| (segment.text, segment.matched))
        .collect();
    assert_eq!(segments, [("Crème ", false), ("brûlé", true), ("e", false)]);
    let push = match_ranges("pu", "Git push").unwrap();
    assert_eq!(&"Git push"[push[0].clone()], "pu");
    assert_eq!(push[0].start, 4);
}

use armature_core::color::{Notation, Rgb};

// color.test.ts
#[test]
fn colour_notations() {
    let red = Rgb::read("red").unwrap();
    assert_eq!(red.write(Notation::Rgb), "rgb(255 0 0)");
    assert_eq!(red.write(Notation::Hex), "#ff0000");
    assert_eq!(red.write(Notation::Hsl), "hsl(0 100% 50%)");
    assert_eq!(red.write(Notation::Oklch), "oklch(0.628 0.2577 29.23)");
    assert_eq!(red.write(Notation::Lab), "lab(54.29 80.8 69.89)");
    assert_eq!(
        Rgb::read("white").unwrap().write(Notation::Oklch),
        "oklch(1 0 0)"
    );
    let (half_red, alpha) = Rgb::read_alpha("rgb(255 0 0 / 0.5)").unwrap();
    assert_eq!(
        half_red.write_alpha(Notation::Rgb, alpha),
        "rgb(255 0 0 / 0.5)"
    );
    let (black, alpha) = Rgb::read_alpha("rgb(0 0 0 / 0.5)").unwrap();
    assert_eq!(black.write_alpha(Notation::Hex, alpha), "#00000080");
    assert!(Rgb::read("#12").is_none());
}

use armature_core::gauge::{Meter, MeterPart};
use armature_core::primitive::Primitive;
use armature_core::selection::{all_toggled, keys_between, range_selection, toggled_keys};

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|key| key.to_string()).collect()
}

// listbox-range.test.ts
#[test]
fn listbox_ranges() {
    let order = ["a", "b", "c", "d", "e", "f"];
    assert_eq!(keys_between(&order, "b", "d"), ["b", "c", "d"]);
    assert_eq!(keys_between(&order, "d", "b"), ["b", "c", "d"]);
    assert_eq!(keys_between(&order, "c", "c"), ["c"]);
    assert!(keys_between(&order, "x", "b").is_empty() && keys_between(&order, "b", "x").is_empty());
    assert_eq!(
        range_selection(&order, "b", &keys(&["a", "f"]), "d", false, |_| true),
        ["b", "c", "d"]
    );
    assert_eq!(
        range_selection(&order, "c", &keys(&["f", "a"]), "e", true, |_| true),
        ["f", "a", "c", "d", "e"]
    );
    assert_eq!(
        range_selection(&order, "b", &keys(&["f"]), "e", true, |_| true),
        ["f", "b", "c", "d", "e"]
    );
    assert_eq!(
        range_selection(&order, "b", &keys(&["f"]), "c", true, |_| true),
        ["f", "b", "c"],
        "a range can shrink"
    );
    assert_eq!(
        range_selection(&order, "a", &[], "d", false, |key| key != "c"),
        ["a", "b", "d"]
    );
    assert_eq!(
        range_selection(&order, "a", &[], "c", false, |key| key != "a"),
        ["b", "c"]
    );
    assert_eq!(toggled_keys(&keys(&["a", "c"]), "b"), ["a", "c", "b"]);
    assert_eq!(toggled_keys(&keys(&["a", "c"]), "a"), ["c"]);
    let selectable = ["a", "b", "d"];
    let all = all_toggled(&keys(&["d"]), &selectable);
    assert_eq!(all, ["d", "a", "b"]);
    assert!(all_toggled(&all, &selectable).is_empty());
    assert_eq!(
        all_toggled(&keys(&["c", "a", "b", "d"]), &selectable),
        ["c"]
    );
    assert_eq!(all_toggled(&keys(&["c"]), &["a"]), ["c", "a"]);
    assert_eq!(all_toggled(&keys(&["c"]), &[]), ["c"]);
}

// meter.test.ts
#[test]
fn meter_readings() {
    // "a value outside the bounds reads as the nearest bound"
    let mut meter = Meter::new(150.0, 0.0, 100.0);
    assert_eq!(
        meter.semantics(MeterPart::Root, None).range.unwrap().now,
        100.0
    );
    assert_eq!(meter.percent(), 100.0);
    // "a value that isn't a number reads as the minimum"
    meter.set_value(vec!["NaN".into()]);
    assert_eq!(
        meter.semantics(MeterPart::Root, None).range.unwrap().now,
        0.0
    );
    assert_eq!(meter.percent(), 0.0);
}

use armature_core::outline::{OutlineRow, Place, depth_range, moved_rows, place_of, subtree_end};

fn outline(text: &str) -> Vec<OutlineRow> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| OutlineRow {
            key: line.trim().to_string(),
            depth: (line.len() - line.trim_start().len()) / 2,
        })
        .collect()
}

// tree-reorder.test.ts, "the pure pieces"
#[test]
fn outline_reordering() {
    let notes = outline("inbox\nprojects\n  site\n    copy\n  app\narchive\n");
    assert_eq!(subtree_end(&notes, 1), 5);
    assert_eq!(subtree_end(&notes, 0), 1);
    assert_eq!(
        place_of(&notes, 4),
        Place {
            parent: Some(1),
            position: 1
        }
    );
    assert_eq!(
        place_of(&notes, 5),
        Place {
            parent: None,
            position: 2
        }
    );
    let row = |key: &str, depth| OutlineRow {
        key: key.into(),
        depth,
    };
    assert_eq!(
        depth_range(Some(&row("a", 1)), Some(&row("b", 0)), None),
        (0, 2)
    );
    assert_eq!(
        depth_range(Some(&row("a", 1)), Some(&row("b", 2)), None),
        (2, 2)
    );
    assert_eq!(depth_range(None, Some(&row("b", 0)), None), (0, 0));
    assert_eq!(depth_range(Some(&row("a", 3)), None, Some(1)), (0, 1));
    assert_eq!(
        moved_rows(&notes, "site", Some("inbox"), 0).unwrap(),
        outline("site\n  copy\ninbox\nprojects\n  app\narchive\n")
    );
    assert_eq!(
        moved_rows(&notes, "app", Some("archive"), 1),
        None,
        "dropped where it was"
    );
}

use armature_core::outline::{OutlineMove, depth_at, move_row, slot_at};

fn written(rows: &[OutlineRow]) -> String {
    rows.iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth), row.key))
        .collect::<Vec<_>>()
        .join("\n")
}

// tree-reorder.test.ts, "with the keyboard" and "with a pointer"
#[test]
fn outline_moves() {
    let notes = outline("inbox\nprojects\n  site\n    copy\n  app\narchive\n");
    let all = |_: &str| true;
    // "Up and Down move an item and its children between rows, and Space drops it in one change"
    let mut moving = OutlineMove::grab(&notes, "site", None).unwrap();
    assert_eq!(moving.keys(), ["site", "copy"]);
    moving.step(true, &all);
    assert_eq!(
        written(moving.rows()),
        "inbox\nprojects\n  app\n  site\n    copy\narchive"
    );
    let landed = moving.dropped(&notes).unwrap();
    assert_eq!(
        (landed.key.as_str(), landed.parent.as_deref(), landed.index),
        ("site", Some("projects"), 1)
    );
    // "Left and Right take an item out a level and in one, as far as the rows around it allow"
    let mut moving = OutlineMove::grab(&notes, "app", None).unwrap();
    moving.indent(false, &all);
    assert!(written(moving.rows()).contains("projects\n  site\n    copy\napp\narchive"));
    moving.indent(false, &all);
    assert!(written(moving.rows()).contains("\napp\narchive"));
    for _ in 0..3 {
        moving.indent(true, &all);
    }
    assert!(
        written(moving.rows()).contains("    copy\n      app\narchive"),
        "a child of copy and no deeper"
    );
    // Escape: the move is forgotten and the rows are as they were.
    // "an item never takes another's children: above a deeper row it goes in as deep"
    let mut moving = OutlineMove::grab(&notes, "inbox", None).unwrap();
    moving.step(true, &all);
    assert!(written(moving.rows()).contains("projects\n  inbox\n  site"));
    moving.step(true, &all);
    assert!(written(moving.rows()).contains("  site\n    inbox\n    copy"));
    moving.indent(false, &all);
    assert!(written(moving.rows()).contains("  site\n    inbox\n    copy"));
    // "moving through a deeper branch comes back out at the depth it aimed for"
    let mut moving = OutlineMove::grab(&notes, "inbox", None).unwrap();
    for _ in 0..5 {
        moving.step(true, &all);
    }
    assert_eq!(
        written(moving.rows()),
        "projects\n  site\n    copy\n  app\narchive\ninbox"
    );
    // "rows inside a collapsed item are skipped, and dropping in one goes after its children"
    let collapsed = |key: &str| !["site", "copy", "app"].contains(&key);
    let mut moving = OutlineMove::grab(&notes, "inbox", None).unwrap();
    moving.step(true, &collapsed);
    moving.indent(true, &collapsed);
    assert_eq!(
        written(moving.rows()),
        "projects\n  site\n    copy\n  app\n  inbox\narchive"
    );
    // "maxDepth caps how deep an item goes"
    let mut moving = OutlineMove::grab(&notes, "archive", Some(1)).unwrap();
    moving.indent(true, &all);
    moving.indent(true, &all);
    assert!(written(moving.rows()).contains("  app\n  archive"));
    // "maxDepth counts the rows under an item too"
    let mut moving = OutlineMove::grab(&notes, "site", Some(2)).unwrap();
    moving.step(true, &all);
    moving.indent(true, &all);
    assert!(written(moving.rows()).contains("  app\n  site\n    copy"));
    let moved = move_row(&notes, "site", Some("archive"), 2, Some(2)).unwrap();
    assert!(written(&moved.rows).contains("  app\n  site\n    copy\narchive"));
    // "move() commits at once and reports the new parent"
    let moved = move_row(&notes, "archive", Some("site"), 1, None).unwrap();
    assert_eq!(
        (moved.parent.as_deref(), moved.index),
        (Some("projects"), 0)
    );
    assert_eq!(
        move_row(&notes, "projects", Some("copy"), 0, None),
        None,
        "not into itself"
    );
    // "a drag moves by the rows' middles and goes in a level for every indent sideways"
    let middles = [10.0, 30.0, 50.0, 70.0, 90.0];
    let mut drag = OutlineMove::grab(&notes, "archive", None).unwrap();
    drag.place(
        slot_at(&middles, 85.0),
        depth_at(0, 58.0 - 10.0, 24.0),
        &all,
    );
    assert!(written(drag.rows()).contains("    copy\n    archive\n  app"));
    drag.place(
        slot_at(&middles, 85.0),
        depth_at(0, 34.0 - 10.0, 24.0),
        &all,
    );
    assert!(written(drag.rows()).contains("    copy\n  archive\n  app"));
    let landed = drag.dropped(&notes).unwrap();
    assert_eq!(
        (landed.parent.as_deref(), landed.index),
        (Some("projects"), 1)
    );
    // "a row dropped where it was" changes nothing
    assert_eq!(
        OutlineMove::grab(&notes, "site", None)
            .unwrap()
            .dropped(&notes),
        None
    );
}

// shortcuts.test.ts
#[test]
fn shortcuts() {
    use armature_core::keys::Modifiers;
    use armature_core::shortcut::{Platform, Shortcut, shortcut_allowed};
    let held = |shift: bool, ctrl: bool, alt: bool| Modifiers { shift, ctrl, alt };
    let is = |text: &str, typed: &str, code: &str, mods: Modifiers| {
        Shortcut::parse(text)
            .unwrap()
            .matches_typed(typed, code, mods)
    };
    // "letters match the typed key, case-insensitively"
    assert!(is("mod+n", "n", "KeyN", held(false, true, false)));
    assert!(is("mod+shift+n", "N", "KeyN", held(true, true, false)));
    // "Option shortcuts on a Mac match by physical key"
    assert!(is("alt+n", "˜", "KeyN", held(false, false, true)));
    assert!(is("alt+n", "Dead", "KeyN", held(false, false, true)));
    assert!(is("alt+1", "¡", "Digit1", held(false, false, true)));
    // "other alphabets match by physical key"
    assert!(is("mod+n", "т", "KeyN", held(false, true, false)));
    // "a Latin letter matches by what it types, so other layouts keep their letters"
    assert!(!is("mod+a", "q", "KeyA", held(false, true, false)));
    assert!(is("mod+a", "a", "KeyQ", held(false, true, false)));
    // "shifted digits match by physical key"
    assert!(is("shift+1", "!", "Digit1", held(true, false, false)));
    // "other keys match by name, with aliases"
    assert!(is(
        "mod+backspace",
        "Backspace",
        "Backspace",
        held(false, true, false)
    ));
    assert!(is("esc", "Escape", "", Modifiers::default()));
    assert!(is("alt+up", "ArrowUp", "", held(false, false, true)));
    assert!(is("ctrl+space", " ", "Space", held(false, true, false)));
    assert!(is("mod+/", "/", "Slash", held(false, true, false)));
    // "modifiers must match exactly"
    assert!(!is("mod+n", "n", "KeyN", held(true, true, false)));
    assert!(!is("mod+n", "n", "KeyN", Modifiers::default()));
    assert!(!is("mod+n", "m", "KeyM", held(false, true, false)));
    // "Apple platforms show glyphs"
    let shown = |text: &str, platform| Shortcut::parse(text).unwrap().format(platform);
    assert_eq!(shown("mod+n", Platform::Mac), "⌘N");
    assert_eq!(shown("alt+n", Platform::Mac), "⌥N");
    assert_eq!(shown("mod+backspace", Platform::Mac), "⌘⌫");
    assert_eq!(shown("shift+enter", Platform::Mac), "⇧↩");
    assert_eq!(shown("esc", Platform::Mac), "⎋");
    assert_eq!(shown("alt+up", Platform::Mac), "⌥↑");
    assert_eq!(shown("mod+down", Platform::Mac), "⌘↓");
    assert_eq!(shown("mod+left", Platform::Mac), "⌘←");
    assert_eq!(shown("mod+right", Platform::Mac), "⌘→");
    // "other platforms show readable names"
    assert_eq!(shown("mod+n", Platform::Other), "Ctrl+N");
    assert_eq!(shown("mod+backspace", Platform::Other), "Ctrl+Backspace");
    assert_eq!(shown("shift+enter", Platform::Other), "Shift+Enter");
    assert_eq!(shown("esc", Platform::Other), "Esc");
    assert_eq!(shown("alt+up", Platform::Other), "Alt+Up");
    assert_eq!(shown("ctrl+space", Platform::Other), "Ctrl+Space");
    // "the text is isolated left to right, so right-to-left layouts don't reorder it"
    assert_eq!(
        Shortcut::parse("mod+n").unwrap().display(Platform::Mac),
        "\u{2066}⌘N\u{2069}"
    );
    assert_eq!(
        Shortcut::parse("mod+n").unwrap().display(Platform::Other),
        "\u{2066}Ctrl+N\u{2069}"
    );
    // "aria-keyshortcuts uses standard key names"
    let aria = |text: &str, platform| Shortcut::parse(text).unwrap().aria(platform);
    assert_eq!(aria("mod+backspace", Platform::Mac), "Meta+Backspace");
    assert_eq!(aria("option+esc", Platform::Other), "Alt+Escape");
    assert_eq!(aria("shift+space", Platform::Mac), "Shift+Space");
    // "a page shortcut runs until a modal opens, and a modal's own shortcut keeps running"
    assert!(shortcut_allowed(Some(0), None));
    assert!(!shortcut_allowed(Some(0), Some(3)));
    assert!(shortcut_allowed(Some(3), Some(3)) && shortcut_allowed(Some(4), Some(3)));
    assert!(!shortcut_allowed(Some(2), Some(3)));
    // "a shortcut of a closed popup never runs"
    assert!(!shortcut_allowed(None, None));
}

// trigger-match.test.ts
#[test]
#[allow(clippy::cloned_ref_to_slice_refs)]
fn trigger_matching() {
    use armature_core::trigger_match::{
        Boundary, Dismissed, Spaces, TriggerRule, find_trigger_match, is_at_boundary,
        is_valid_query, look_behind, map_dismissed, normalize_trigger_text,
    };
    let mention = || TriggerRule::new("@");
    let found = |text: &str, rules: &[TriggerRule], offset: usize| {
        find_trigger_match(text, offset, rules)
            .map(|m| format!("{}{}@{}", m.trigger, m.query, m.start))
    };
    let matched = |text: &str| found(text, &[mention()], 0);
    for (text, expected) in [
        ("@", "@@0"),
        ("@al", "@al@0"),
        ("hi @al", "@al@3"),
        ("hi @José", "@José@3"),
        ("hi @山田", "@山田@3"),
        ("@mañana", "@mañana@0"),
        ("(@al", "@al@1"),
        ("[@al", "@al@1"),
        ("\"@al", "@al@1"),
        ("\u{FFFC}@al", "@al@1"),
        ("line\n@al", "@al@5"),
        ("a.b-c_d @x.y-z_w", "@x.y-z_w@8"),
    ] {
        assert_eq!(matched(text).as_deref(), Some(expected), "{text:?}");
    }
    for text in ["mail@al", "@ al", "@al ", "@al\n", "@a!b", "hello", ""] {
        assert_eq!(matched(text), None, "{text:?}");
    }
    // "the nearest trigger before the caret wins"
    assert_eq!(matched("@ann @bo").as_deref(), Some("@bo@5"));
    // "a full-width trigger an input method committed matches"
    assert_eq!(matched("＠al").as_deref(), Some("@al@0"));
    assert_eq!(
        found("x ／m", &[TriggerRule::new("/")], 0).as_deref(),
        Some("/m@2")
    );
    // "boundary start matches only at offset 0, so a leading space escapes it"
    let command = TriggerRule {
        boundary: Boundary::Start,
        ..TriggerRule::new("/")
    };
    assert_eq!(
        found("/mod", &[command.clone()], 0).as_deref(),
        Some("/mod@0")
    );
    assert_eq!(found(" /mod", &[command.clone()], 0), None);
    assert_eq!(found("a /mod", &[command], 0), None);
    // "boundary line matches at the start of any line"
    let line = TriggerRule {
        boundary: Boundary::Line,
        ..TriggerRule::new("/")
    };
    assert_eq!(
        found("one\n/x", &[line.clone()], 0).as_deref(),
        Some("/x@4")
    );
    assert_eq!(found("one /x", &[line], 0), None);
    // "boundary anywhere matches mid-word"
    assert_eq!(
        found(
            "mail@al",
            &[TriggerRule {
                boundary: Boundary::Anywhere,
                ..mention()
            }],
            0
        )
        .as_deref(),
        Some("@al@4")
    );
    // "spaces allow phrases, and a number caps the words"
    let phrases = TriggerRule {
        spaces: Spaces::Any,
        ..mention()
    };
    assert_eq!(
        found("@next wed", &[phrases.clone()], 0).as_deref(),
        Some("@next wed@0")
    );
    assert_eq!(
        found("@next ", &[phrases.clone()], 0).as_deref(),
        Some("@next @0")
    );
    assert_eq!(found("@a  b", &[phrases], 0), None);
    let two = TriggerRule {
        spaces: Spaces::Words(2),
        ..mention()
    };
    assert_eq!(found("@a b", &[two.clone()], 0).as_deref(), Some("@a b@0"));
    assert_eq!(
        found("@a b ", &[two.clone()], 0).as_deref(),
        Some("@a b @0")
    );
    assert_eq!(found("@a b c", &[two], 0), None);
    // "a multi-character trigger matches"
    assert_eq!(
        found("see [[Page", &[TriggerRule::new("[[")], 0).as_deref(),
        Some("[[Page@4")
    );
    // "triggers with regex characters are matched literally"
    assert_eq!(
        found("x ]a", &[TriggerRule::new("]")], 0).as_deref(),
        Some("]a@2")
    );
    let lower = TriggerRule {
        accepts: Some(|ch| ch.is_ascii_lowercase()),
        ..TriggerRule::new("-")
    };
    assert_eq!(found("x -a", &[lower], 0).as_deref(), Some("-a@2"));
    assert_eq!(
        found("x .*a", &[TriggerRule::new(".*")], 0).as_deref(),
        Some(".*a@2")
    );
    // "maxLength ends a long query"
    let short = TriggerRule {
        max_length: 3,
        ..mention()
    };
    assert_eq!(found("@abcd", &[short.clone()], 0), None);
    assert_eq!(found("@abc", &[short], 0).as_deref(), Some("@abc@0"));
    // "the offset says where the text starts, and an unknown character before it is no boundary"
    assert_eq!(found(" @al", &[mention()], 10).as_deref(), Some("@al@11"));
    assert_eq!(found("@al", &[mention()], 10), None);
    // "a token reads as U+FFFC and ends a query"
    assert_eq!(matched("@al\u{FFFC}b"), None);
    // "reports which rule matched"
    assert_eq!(
        find_trigger_match("hi /x", 0, &[mention(), TriggerRule::new("/")])
            .unwrap()
            .rule,
        1
    );
    // "isValidQuery and isAtBoundary"
    assert!(is_valid_query("", &mention()));
    let chars = |text: &str| text.chars().collect::<Vec<_>>();
    assert!(is_at_boundary(&chars("a @"), 2, 0, Boundary::Word));
    assert!(!is_at_boundary(&chars("a@"), 1, 0, Boundary::Word));
    assert_eq!(
        look_behind(&[
            mention(),
            TriggerRule {
                max_length: 80,
                ..TriggerRule::new("[[")
            }
        ]),
        83
    );
    assert_eq!(normalize_trigger_text("＠＃／"), "@#/");
    // mapDismissed
    let dismissed = Dismissed {
        trigger: "@".into(),
        start: 3,
        end: 6,
    };
    let at = |start, end| {
        Some(Dismissed {
            trigger: "@".into(),
            start,
            end,
        })
    };
    assert_eq!(map_dismissed(&dismissed, 0, 0, 2), at(5, 8));
    assert_eq!(map_dismissed(&dismissed, 0, 2, 0), at(1, 4));
    assert_eq!(map_dismissed(&dismissed, 6, 6, 3), at(3, 9));
    assert_eq!(map_dismissed(&dismissed, 5, 6, 0), at(3, 5));
    assert_eq!(map_dismissed(&dismissed, 3, 4, 0), None);
    assert_eq!(map_dismissed(&dismissed, 2, 4, 1), None);
    assert_eq!(map_dismissed(&dismissed, 10, 12, 5), at(3, 6));
    // "random edits never leave a range that ends before its trigger"
    let mut seed: u64 = 7;
    let mut random = || {
        seed = (seed * 1_103_515_245 + 12_345) % (1 << 31);
        seed as f64 / (1u64 << 31) as f64
    };
    for _ in 0..2000 {
        let mut range = Some(Dismissed {
            trigger: "@".into(),
            start: 5,
            end: 9,
        });
        for _ in 0..10 {
            let Some(current) = range.clone() else { break };
            let start = (random() * 20.0) as usize;
            let end = start + (random() * 4.0) as usize;
            range = map_dismissed(&current, start, end, (random() * 4.0) as usize);
            if let Some(range) = &range {
                assert!(range.end > range.start);
            }
        }
    }
}

// token-input-model.test.ts
#[test]
fn token_text_model() {
    use armature_core::token_text::*;
    let alice = || Token::new("mention", "alice", "@Alice").with_id("a");
    let file = || Token::new("file", "src/a.ts", "a.ts").with_id("f");
    let text = |text: &str| Segment::Text(text.to_string());
    let token = |segment: &Segment| match segment {
        Segment::Token(token) => token.clone(),
        Segment::Text(_) => panic!("expected a token"),
    };
    // "normalizing merges strings, drops empty ones and gives tokens ids"
    let value = normalize(vec![
        "a".into(),
        "".into(),
        "b".into(),
        Token::new("x", "1", "One").into(),
        "".into(),
        "c".into(),
    ]);
    assert_eq!(value.len(), 3);
    assert_eq!(value[0], text("ab"));
    let made = token(&value[1]).id;
    assert!(made.starts_with('t') && made[1..].chars().all(|ch| ch.is_ascii_digit()));
    assert_eq!(normalize(vec![alice().into()])[0], Segment::Token(alice()));
    // "a repeated id stays with the first token, and the next gets a new one"
    let pair = normalize(vec![alice().into(), alice().into()]);
    assert_eq!(token(&pair[0]).id, "a");
    assert_ne!(token(&pair[1]).id, "a");
    // "a supplied id too large to count past leaves generated ids unique"
    normalize(vec![
        Token::new("file", "x", "x")
            .with_id("t99999999999999999999")
            .into(),
    ]);
    let first = token(&normalize(vec![Token::new("file", "x", "x").into()])[0]).id;
    let second = token(&normalize(vec![Token::new("file", "x", "x").into()])[0]).id;
    assert_ne!(first, second);
    // "a token counts as one offset"
    let value = normalize(vec![
        "hi ".into(),
        alice().into(),
        " and ".into(),
        file().into(),
    ]);
    assert_eq!(value_length(&value), 10);
    assert_eq!(offset_text(&value), "hi \u{FFFC} and \u{FFFC}");
    assert_eq!(token_at(&value, 3), Some(&alice()));
    assert_eq!(offset_of_token(&value, "f"), Some(9));
    // "slicing keeps whole tokens"
    let value = normalize(vec!["hi ".into(), alice().into(), "!".into()]);
    assert_eq!(slice_value(&value, 2, 4), [text(" "), alice().into()]);
    assert_eq!(slice_value(&value, 4, usize::MAX), [text("!")]);
    // "an edit over a token removes all of it, and edits before it shift it"
    let value = normalize(vec!["ab".into(), alice().into(), "cd".into()]);
    assert_eq!(
        replace_range(&value, 1, 3, vec!["X".into()]),
        [text("aXcd")]
    );
    assert_eq!(
        offset_of_token(&replace_range(&value, 0, 0, vec!["zz".into()]), "a"),
        Some(4)
    );
    assert_eq!(
        removed_tokens(&value, &replace_range(&value, 2, 3, vec![])),
        [alice()]
    );
    // "word deletion stops at tokens, and removes just the token when it's next"
    let value = normalize(vec!["one ".into(), alice().into(), "two three".into()]);
    assert_eq!(word_boundary(&value, 14, false), 9);
    assert_eq!(word_boundary(&value, 8, false), 5);
    assert_eq!(word_boundary(&value, 5, false), 4);
    assert_eq!(word_boundary(&value, 4, false), 0);
    assert_eq!(word_boundary(&value, 4, true), 5);
    // "line deletion goes to the start of the line"
    let value = normalize(vec!["one\ntwo".into()]);
    assert_eq!(line_boundary(&value, 7, false), 4);
    assert_eq!(line_boundary(&value, 4, false), 3);
    assert_eq!(line_boundary(&value, 0, true), 3);
    // "tokens are written as links and labels as text"
    let value = normalize(vec![
        "Summarize ".into(),
        file().into(),
        " for ".into(),
        alice().into(),
    ]);
    assert_eq!(
        to_text(&value),
        "Summarize [@a.ts](file:src/a.ts) for [@@Alice](mention:alice)"
    );
    assert_eq!(to_label(&value), "Summarize a.ts for @Alice");
    // Compares content, since reading text back makes new ids.
    let strip = |value: &[Segment]| -> Vec<Segment> {
        value
            .iter()
            .map(|segment| match segment {
                Segment::Token(token) => Segment::Token(Token {
                    id: String::new(),
                    ..token.clone()
                }),
                other => other.clone(),
            })
            .collect()
    };
    // "labels and values with brackets and parentheses round-trip"
    let odd = Token::new("file", "a (copy).ts", "[draft] a\\b").with_id("x");
    assert_eq!(
        strip(&from_text(&to_text(&[odd.clone().into()]))),
        strip(&[odd.into()])
    );
    // "text that looks like a token stays text"
    let value = normalize(vec!["see [@x](y:z) and \\[@q](r:s)".into()]);
    assert_eq!(from_text(&to_text(&value)), value);
    // "malformed links are text"
    assert_eq!(from_text("[@open](type:val"), [text("[@open](type:val")]);
    assert_eq!(from_text("[@a](b c:d)"), [text("[@a](b c:d)")]);
    // "any value round-trips through the codec"
    let mut seed: u64 = 42;
    let mut random = || {
        seed = (seed * 1_103_515_245 + 12_345) % (1 << 31);
        seed as f64 / (1u64 << 31) as f64
    };
    let alphabet = [
        "a", "b", " ", "\n", "[", "]", "(", ")", "@", "\\", ":", "é", "山", "[@", "\\[@",
    ];
    let kinds = ["mention", "file", "skill-run", "a_b"];
    for run in 0..3000 {
        let words = |random: &mut dyn FnMut() -> f64| -> String {
            let count = (random() * 8.0) as usize;
            (0..count)
                .map(|_| alphabet[(random() * alphabet.len() as f64) as usize])
                .collect()
        };
        let mut segments: Vec<Segment> = Vec::new();
        for _ in 0..(random() * 6.0) as usize {
            if random() < 0.5 {
                segments.push(Segment::Text(words(&mut random)));
            } else {
                let kind = kinds[(random() * kinds.len() as f64) as usize];
                let value = words(&mut random);
                let label = words(&mut random);
                // Upstream draws twice more here, for a status and for data.
                random();
                random();
                segments.push(Token::new(kind, &value, &label).into());
            }
        }
        let value = normalize(segments);
        assert_eq!(
            strip(&from_text(&to_text(&value))),
            strip(&value),
            "run {run}: {value:?}"
        );
    }
}

// command.test.ts, "fuzzy scores"
#[test]
fn fuzzy_scores() {
    use armature_core::text_match::fuzzy_score;
    // "the whole text beats a prefix, which beats a word, a substring and scattered letters"
    let scores: Vec<u32> = [
        "Set",
        "Settings",
        "Quick setup",
        "Assets",
        "Show emoji teams",
    ]
    .iter()
    .map(|text| fuzzy_score("set", text).unwrap_or(0))
    .collect();
    let mut sorted = scores.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(scores, sorted, "best first");
    assert!(scores.iter().all(|score| *score > 0));
    assert!(
        scores.windows(2).all(|pair| pair[0] > pair[1]),
        "each strictly beats the next: {scores:?}"
    );
    // "letters in order match across words, and word starts count most"
    assert!(fuzzy_score("gp", "Git push") > fuzzy_score("gp", "Grep"));
    assert!(fuzzy_score("git ps", "git push").is_some());
    assert_eq!(fuzzy_score("xyz", "Calendar"), None);
    // "case and accents are ignored"
    assert_eq!(fuzzy_score("rac", "Calendar"), None);
    assert!(fuzzy_score("ECL", "Éclair").unwrap() > 900);
}

// roving.test.ts
#[test]
fn roving_tab_stops() {
    use armature_core::keys::Key;
    use armature_core::roving::{FieldCaret, Stop, field_keeps_key, roving_stop};
    let item = |key| Stop {
        key,
        disabled: false,
        focusable: true,
        preferred: false,
    };
    // "a remembered item keeps it while it can take focus"
    assert_eq!(roving_stop(&[item("a"), item("b")], Some("b")), Some("b"));
    assert_eq!(
        roving_stop(
            &[
                item("a"),
                Stop {
                    focusable: false,
                    ..item("b")
                }
            ],
            Some("b")
        ),
        Some("a")
    );
    assert_eq!(roving_stop(&[item("a")], Some("gone")), Some("a"));
    // "otherwise the first preferred enabled item, then the first enabled one"
    assert_eq!(
        roving_stop(
            &[
                item("a"),
                Stop {
                    preferred: true,
                    ..item("b")
                }
            ],
            None
        ),
        Some("b")
    );
    assert_eq!(
        roving_stop(
            &[
                item("a"),
                Stop {
                    preferred: true,
                    disabled: true,
                    ..item("b")
                }
            ],
            None
        ),
        Some("a")
    );
    assert_eq!(
        roving_stop(
            &[
                Stop {
                    disabled: true,
                    ..item("a")
                },
                item("b")
            ],
            None
        ),
        Some("b")
    );
    // "a group where every item is disabled keeps a focusable one, unless none can take focus"
    assert_eq!(
        roving_stop(
            &[
                Stop {
                    disabled: true,
                    ..item("a")
                },
                Stop {
                    disabled: true,
                    ..item("b")
                }
            ],
            None
        ),
        Some("a")
    );
    assert_eq!(
        roving_stop(
            &[Stop {
                disabled: true,
                focusable: false,
                ..item("a")
            }],
            None
        ),
        None
    );
    // "Left and Right leave only from the end they point at, with nothing selected"
    let field = |start, end, rtl| FieldCaret {
        selection: Some((start, end)),
        length: 5,
        rtl,
        uses_up_down: false,
    };
    assert!(!field_keeps_key(Key::ArrowRight, field(5, 5, false)));
    assert!(field_keeps_key(Key::ArrowRight, field(2, 2, false)));
    assert!(!field_keeps_key(Key::ArrowLeft, field(0, 0, false)));
    assert!(field_keeps_key(Key::ArrowLeft, field(0, 3, false)));
    // "right to left, the arrows swap"
    assert!(!field_keeps_key(Key::ArrowLeft, field(5, 5, true)));
    assert!(!field_keeps_key(Key::ArrowRight, field(0, 0, true)));
    assert!(field_keeps_key(Key::ArrowRight, field(5, 5, true)));
    // "Home and End stay with the field, and Up and Down only with fields that use them"
    assert!(field_keeps_key(Key::Home, field(0, 0, false)));
    assert!(field_keeps_key(Key::End, field(5, 5, false)));
    assert!(!field_keeps_key(Key::ArrowUp, field(0, 0, false)));
    assert!(field_keeps_key(
        Key::ArrowUp,
        FieldCaret {
            uses_up_down: true,
            ..field(0, 0, false)
        }
    ));
    // "fields without a caret never keep arrows"
    assert!(!field_keeps_key(
        Key::ArrowRight,
        FieldCaret {
            selection: None,
            length: 0,
            rtl: false,
            uses_up_down: false
        }
    ));
}

// locale.test.ts, "locale tags"
#[test]
fn locale_tags() {
    use armature_core::locale::to_locale;
    // "keeps a tag Intl accepts, canonicalized"
    assert_eq!(to_locale("en-US").as_deref(), Some("en-US"));
    assert_eq!(to_locale("de").as_deref(), Some("de"));
    assert_eq!(to_locale("EN-us").as_deref(), Some("en-US"));
    // "reads POSIX tags, with their encoding and modifier dropped"
    assert_eq!(to_locale("en_US").as_deref(), Some("en-US"));
    assert_eq!(to_locale("en-US@posix").as_deref(), Some("en-US"));
    assert_eq!(to_locale("de_DE.UTF-8@euro").as_deref(), Some("de-DE"));
    // "gives undefined for a tag Intl rejects, or none"
    assert_eq!(to_locale("not a locale!"), None);
    assert_eq!(to_locale(""), None);
}

// resizable.test.ts: sizes, settling, moving a handle, fitting a layout, panels and keys
#[test]
fn panel_layouts() {
    use armature_core::panel_layout::*;
    let panel = PanelConstraints::default;
    let moved = |layout: &[f64], panels: &[PanelConstraints], handle, delta| {
        resize_at(layout, panels, handle, delta, 0.5, &[])
    };
    // "numbers are percentages, and pixel and rem sizes resolve against the space"
    assert_eq!(to_percent(PanelSize::Percent(30.0), 0.0, 16.0), Some(30.0));
    assert_eq!(to_percent(PanelSize::Px(200.0), 800.0, 16.0), Some(25.0));
    assert_eq!(to_percent(PanelSize::Rem(10.0), 800.0, 20.0), Some(25.0));
    // "pixel sizes stay unknown until the group is measured"
    assert_eq!(to_percent(PanelSize::Px(200.0), 0.0, 16.0), None);
    assert_eq!(
        constraints_of(
            PanelSizes {
                min_size: Some(PanelSize::Px(200.0)),
                ..Default::default()
            },
            0.0,
            16.0
        )
        .min,
        0.0
    );
    // "constraints keep collapsed at most the minimum and the maximum at least the minimum"
    let odd = PanelSizes {
        min_size: Some(PanelSize::Percent(30.0)),
        max_size: Some(PanelSize::Percent(20.0)),
        collapsed_size: Some(PanelSize::Percent(40.0)),
        ..Default::default()
    };
    assert_eq!(
        constraints_of(odd, 1000.0, 16.0),
        PanelConstraints {
            min: 30.0,
            max: 30.0,
            collapsible: false,
            collapsed: 30.0,
            initial: None
        }
    );
    // settling
    let sidebar = PanelConstraints {
        min: 20.0,
        collapsible: true,
        ..panel()
    };
    assert_eq!(
        (
            settle(&sidebar, 11.0, 30.0, 0.5),
            settle(&sidebar, 9.0, 30.0, 0.5)
        ),
        (20.0, 0.0)
    );
    assert_eq!(
        (
            settle(&sidebar, 9.0, 0.0, 0.5),
            settle(&sidebar, 11.0, 0.0, 0.5)
        ),
        (0.0, 20.0)
    );
    assert_eq!(
        (
            settle(&sidebar, 19.0, 20.0, 1.0),
            settle(&sidebar, 1.0, 0.0, 1.0)
        ),
        (0.0, 20.0)
    );
    assert_eq!(
        settle(
            &PanelConstraints {
                min: 20.0,
                ..panel()
            },
            5.0,
            30.0,
            0.5
        ),
        20.0
    );
    // "the panels on either side give and take the same amount"
    assert_eq!(
        moved(&[30.0, 70.0], &[panel(), panel()], 0, 10.0),
        Some(vec![40.0, 60.0])
    );
    assert_eq!(
        moved(&[30.0, 70.0], &[panel(), panel()], 0, -10.0),
        Some(vec![20.0, 80.0])
    );
    // "returns the same array when nothing can move"
    assert_eq!(
        moved(
            &[30.0, 70.0],
            &[
                PanelConstraints {
                    max: 30.0,
                    ..panel()
                },
                panel()
            ],
            0,
            10.0
        ),
        None
    );
    assert_eq!(moved(&[30.0, 70.0], &[panel(), panel()], 0, 0.0), None);
    assert_eq!(moved(&[30.0, 70.0], &[panel(), panel()], 1, 10.0), None);
    // "a panel at its minimum passes the rest on to the next one"
    let three = [
        panel(),
        PanelConstraints {
            min: 20.0,
            ..panel()
        },
        panel(),
    ];
    assert_eq!(
        moved(&[20.0, 30.0, 50.0], &three, 0, 30.0),
        Some(vec![50.0, 20.0, 30.0])
    );
    // "a disabled handle stops the cascade either way, so the panels past it keep their sizes"
    let four = [
        panel(),
        PanelConstraints {
            min: 20.0,
            ..panel()
        },
        panel(),
        PanelConstraints {
            min: 10.0,
            ..panel()
        },
    ];
    let layout = [20.0, 30.0, 30.0, 20.0];
    let locked = [false, true];
    assert_eq!(
        resize_at(&layout, &four, 0, 30.0, 0.5, &locked),
        Some(vec![30.0, 20.0, 30.0, 20.0])
    );
    assert_eq!(
        resize_at(&layout, &four, 2, -40.0, 0.5, &locked),
        Some(vec![20.0, 30.0, 0.0, 50.0])
    );
    assert_eq!(
        resize_at(&[20.0, 30.0, 0.0, 50.0], &four, 2, -5.0, 0.5, &locked),
        None
    );
    assert_eq!(
        moved(&layout, &four, 0, 30.0),
        Some(vec![50.0, 20.0, 10.0, 20.0])
    );
    // "a handle's bounds stop at a disabled handle"
    assert_eq!(
        handle_bounds(&[20.0, 30.0, 50.0], &three, 0, &locked),
        (0.0, 30.0)
    );
    assert_eq!(
        handle_bounds(&[20.0, 30.0, 50.0], &three, 0, &[]),
        (0.0, 80.0)
    );
    // "the growing panel stops at its maximum"
    assert_eq!(
        moved(
            &[20.0, 80.0],
            &[
                PanelConstraints {
                    max: 40.0,
                    ..panel()
                },
                panel()
            ],
            0,
            50.0
        ),
        Some(vec![40.0, 60.0])
    );
    // "a shrinking panel collapses past half the way, and the other takes all it frees"
    let shutting = [sidebar, panel()];
    assert_eq!(
        moved(&[30.0, 70.0], &shutting, 0, -15.0),
        Some(vec![20.0, 80.0])
    );
    assert_eq!(
        moved(&[30.0, 70.0], &shutting, 0, -21.0),
        Some(vec![0.0, 100.0])
    );
    // "dragging a collapsed panel open snaps it to its minimum"
    assert_eq!(moved(&[0.0, 100.0], &shutting, 0, 5.0), None);
    assert_eq!(
        moved(&[0.0, 100.0], &shutting, 0, 12.0),
        Some(vec![20.0, 80.0])
    );
    // "a collapse that frees more than the other panel can take stops at the minimum"
    assert_eq!(
        moved(
            &[30.0, 70.0],
            &[
                sidebar,
                PanelConstraints {
                    max: 75.0,
                    ..panel()
                }
            ],
            0,
            -25.0
        ),
        Some(vec![25.0, 75.0])
    );
    // "a collapsed panel stays shut when its neighbours can't free its minimum"
    let tight = [
        PanelConstraints {
            min: 30.0,
            collapsible: true,
            ..panel()
        },
        PanelConstraints {
            min: 80.0,
            ..panel()
        },
    ];
    assert_eq!(moved(&[0.0, 100.0], &tight, 0, 40.0), None);
    // "Home and End go as far as the panels allow"
    let ends = [
        PanelConstraints {
            min: 10.0,
            max: 60.0,
            collapsible: true,
            ..panel()
        },
        PanelConstraints {
            min: 30.0,
            ..panel()
        },
    ];
    assert_eq!(handle_bounds(&[30.0, 70.0], &ends, 0, &[]), (0.0, 60.0));
    assert_eq!(
        handle_bounds(
            &[30.0, 70.0],
            &[
                PanelConstraints {
                    min: 10.0,
                    ..panel()
                },
                PanelConstraints {
                    min: 50.0,
                    ..panel()
                }
            ],
            0,
            &[]
        ),
        (10.0, 50.0)
    );
    // "every handle's bounds at once work each handle out alone when a panel can collapse or is out of its bounds"
    assert_eq!(
        all_handle_bounds(&[30.0, 70.0], &shutting, &[]),
        [(0.0, 100.0)]
    );
    assert_eq!(
        all_handle_bounds(
            &[5.0, 95.0],
            &[
                PanelConstraints {
                    min: 10.0,
                    ..panel()
                },
                panel()
            ],
            &[]
        ),
        [(10.0, 100.0)]
    );
    // "every handle's bounds at once match each handle's own", with upstream's seeded generator
    let mut seed: u64 = 7;
    let mut random = || {
        seed = seed * 16807 % 2_147_483_647;
        (seed - 1) as f64 / 2_147_483_646.0
    };
    let mut one_pass = 0;
    for run in 0..500 {
        let count = 2 + (random() * 7.0) as usize;
        let collapsing = run % 4 == 0;
        let bound = |random: &mut dyn FnMut() -> f64, most: f64| {
            if random() < 0.5 {
                (random() * most).round()
            } else {
                random() * most
            }
        };
        let panels: Vec<PanelConstraints> = (0..count)
            .map(|_| {
                let min = if random() < 0.4 {
                    0.0
                } else {
                    bound(&mut random, 15.0)
                };
                let collapsible = collapsing && random() < 0.5;
                let max = if random() < 0.4 {
                    100.0
                } else {
                    (min + bound(&mut random, 60.0)).min(100.0)
                };
                let collapsed = if collapsible && random() < 0.5 {
                    min / 2.0
                } else {
                    0.0
                };
                PanelConstraints {
                    min,
                    max,
                    collapsible,
                    collapsed,
                    initial: None,
                }
            })
            .collect();
        let wanted: Vec<f64> = panels.iter().map(|_| random() * 100.0).collect();
        let mut layout = fit_layout(&wanted, &panels);
        for handle in 0..count - 1 {
            let pick = random();
            let delta = if pick < 0.15 {
                home_delta(&layout, &panels, handle)
            } else if pick < 0.3 {
                end_delta(&layout, &panels, handle)
            } else {
                continue;
            };
            layout = resize_at(&layout, &panels, handle, delta, 1.0, &[]).unwrap_or(layout);
        }
        if run % 10 == 5 {
            layout = vec![100.0 / count as f64; count];
        }
        if panels.iter().zip(&layout).all(|(one, size)| {
            !one.collapsible && *size >= one.min - 1e-6 && *size <= one.max + 1e-6
        }) {
            one_pass += 1;
        }
        let locked: Vec<bool> = panels.iter().map(|_| random() < 0.2).collect();
        let all = all_handle_bounds(&layout, &panels, &locked);
        assert_eq!(all.len(), count - 1);
        for (handle, bounds) in all.iter().enumerate() {
            let one = handle_bounds(&layout, &panels, handle, &locked);
            assert!(
                (bounds.0 - one.0).abs() < 1e-9 && (bounds.1 - one.1).abs() < 1e-9,
                "run {run}, handle {handle}: {bounds:?} against {one:?}"
            );
        }
    }
    assert!(one_pass > 250, "most runs take the one pass: {one_pass}");
    // fitting a layout
    assert_eq!(fit_layout(&[1.0, 3.0], &[panel(), panel()]), [25.0, 75.0]);
    assert_eq!(
        fit_layout(
            &[10.0, 90.0],
            &[
                PanelConstraints {
                    min: 20.0,
                    ..panel()
                },
                panel()
            ]
        ),
        [20.0, 80.0]
    );
    assert_eq!(
        fit_layout(
            &[50.0, 50.0],
            &[
                PanelConstraints {
                    max: 30.0,
                    ..panel()
                },
                panel()
            ]
        ),
        [30.0, 70.0]
    );
    assert_eq!(
        fit_layout(&[0.0, 50.0, 50.0], &[sidebar, panel(), panel()]),
        [0.0, 50.0, 50.0]
    );
    // "panels without a default share what's left evenly"
    assert_eq!(
        initial_layout(&[
            PanelConstraints {
                initial: Some(40.0),
                ..panel()
            },
            panel(),
            panel()
        ]),
        [40.0, 30.0, 30.0]
    );
    let thirds = initial_layout(&[panel(), panel(), panel()]);
    assert!(thirds.iter().all(|size| (size - 100.0 / 3.0).abs() < 1e-3));
    // "panels that stay keep their sizes relative to each other when one comes"
    let grown = carry_layout(
        &[("a", 25.0), ("b", 75.0)],
        &["a", "b", "c"],
        &[
            panel(),
            panel(),
            PanelConstraints {
                initial: Some(20.0),
                ..panel()
            },
        ],
    );
    assert_eq!(grown, [20.0, 60.0, 20.0]);
    assert_eq!(
        carry_layout(
            &[("a", 20.0), ("b", 60.0), ("c", 20.0)],
            &["a", "b"],
            &[panel(), panel()]
        ),
        [25.0, 75.0]
    );
    // "resizing a panel goes through the handle after it, or before it for the last"
    let plain = [panel(), panel(), panel()];
    assert_eq!(
        resize_panel(&[30.0, 40.0, 30.0], &plain, 0, 20.0, 0.5, &[]),
        Some(vec![20.0, 50.0, 30.0])
    );
    assert_eq!(
        resize_panel(&[30.0, 40.0, 30.0], &plain, 2, 40.0, 0.5, &[]),
        Some(vec![30.0, 30.0, 40.0])
    );
    // "Enter collapses the panel before a handle, or the one after when only it can"
    let can = PanelConstraints {
        collapsible: true,
        ..panel()
    };
    assert_eq!(toggle_target(&[can, panel()], 0), Some(0));
    assert_eq!(toggle_target(&[panel(), can], 0), Some(1));
    assert_eq!(toggle_target(&[panel(), panel()], 0), None);
}
