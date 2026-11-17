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
