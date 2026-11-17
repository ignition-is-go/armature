//! The components and options added after comparing every option with UI SDK's.

use armature_core::calendar::{self, Calendar, Selecting};
use armature_core::combobox::{self, Combobox};
use armature_core::command::{self, Command};
use armature_core::date::Date;
use armature_core::date_input::{self, DateInput};
use armature_core::event_list::{self, EventList};
use armature_core::geometry::Size;
use armature_core::image_cropper::{self, ImageCropper};
use armature_core::keys::Dir;
use armature_core::number::NumberRange;
use armature_core::number_input::{self, NumberInput};
use armature_core::panel_group::{self, PanelGroup};
use armature_core::range_slider::{self, RangeSlider};
use armature_core::rating::{self, Rating};
use armature_core::select::{self, Select};
use armature_core::tree::tree_from_paths;
use armature_core::tree_view::{self, TreeView};
use armature_core::trigger_menu::{self, TriggerMenu};
use armature_leptos::{Part, Root, ValueChange, use_primitive};
use leptos::prelude::*;

use crate::{Section, follow};

#[component]
fn Grips() -> impl IntoView {
    let slider = use_primitive::<RangeSlider>();
    let left = move |at: usize| move || format!("{}%", slider.with(|s| s.fractions()[at]) * 100.0);
    view! {
        <Part part=range_slider::Part::Track>
            <div class="thumb-slot" style:left=left(0)><Part part=range_slider::Part::Thumb item="0" label="Lowest price" /></div>
            <div class="thumb-slot" style:left=left(1)><Part part=range_slider::Part::Thumb item="1" label="Highest price" /></div>
        </Part>
    }
}

#[component]
fn Panes() -> impl IntoView {
    let group = use_primitive::<PanelGroup>();
    let share = move |at: usize| move || format!("{}%", group.with(|g| g.sizes()[at]));
    view! {
        <Part part=panel_group::Part::Root>
            <div class="panes">
                <div style:width=share(0)><Part part=panel_group::Part::Panel item="0">"A"</Part></div>
                <Part part=panel_group::Part::Handle item="0" label="Resize A and B" />
                <div style:width=share(1)><Part part=panel_group::Part::Panel item="1">"B"</Part></div>
                <Part part=panel_group::Part::Handle item="1" label="Resize B and C" />
                <div style:width=share(2)><Part part=panel_group::Part::Panel item="2">"C"</Part></div>
            </div>
        </Part>
    }
}

#[component]
fn Stars() -> impl IntoView {
    let rating = use_primitive::<Rating>();
    view! {
        <Part part=rating::Part::Root>
            <div class="stars">
                {(1..=5u32)
                    .map(|star| {
                        let fill = move || format!("{}%", rating.with(|r| r.fill(star)) * 100.0);
                        view! {
                            <Part part=rating::Part::Star item=star.to_string()>
                                <span class="star"><span class="star-fill" style:width=fill>"★"</span>"☆"</span>
                            </Part>
                        }
                    })
                    .collect_view()}
            </div>
        </Part>
    }
}

#[component]
fn TwoMonths() -> impl IntoView {
    let calendar = use_primitive::<Calendar>();
    let months = move || calendar.with(|c| c.shown_months());
    view! {
        <Part part=calendar::Part::PreviousMonth label="Earlier">"‹"</Part>
        <Part part=calendar::Part::NextMonth label="Later">"›"</Part>
        <Part part=calendar::Part::Grid>
            <div class="months">
                <For each=months key=|month| *month let:month>
                    <div class="month">
                        <p class="hint">{format!("{}-{:02}", month.0, month.1)}</p>
                        {calendar
                            .with_untracked(|c| c.weeks_of(month.0, month.1))
                            .into_iter()
                            .map(|week| {
                                view! {
                                    <div class="week">
                                        {week
                                            .map(|day| {
                                                // A day is drawn once, in its own month.
                                                if (day.year, day.month) == month {
                                                    view! { <Part part=calendar::Part::Day item=day.format()>{day.day}</Part> }.into_any()
                                                } else {
                                                    view! { <span class="blank"></span> }.into_any()
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                </For>
            </div>
        </Part>
    }
}

#[component]
fn LazyRows() -> impl IntoView {
    let tree = use_primitive::<TreeView>();
    view! {
        <Part part=tree_view::Part::Root>
            <For each=move || tree.with(|t| t.rows()) key=|row| row.key.clone() let:row>
                {
                    let (key, name) = (row.key.clone(), row.key.rsplit('/').next().unwrap_or(&row.key).to_string());
                    view! {
                        <div style:padding-left=format!("{}px", (row.level - 1) * 16)>
                            <Part part=tree_view::Part::Node item=key>{name}</Part>
                        </div>
                    }
                }
            </For>
        </Part>
    }
}

#[component]
fn ZoomCrop() -> impl IntoView {
    let cropper = use_primitive::<ImageCropper>();
    let side = move |at: usize| move || format!("{}%", cropper.with(|c| c.fractions()[at]) * 100.0);
    view! {
        <p id="zoom-value">"Zoom: " {move || format!("{:.2}", cropper.with(|c| c.zoom()))}</p>
        <Part part=image_cropper::Part::Area>
            <div class="crop-box" style:left=side(0) style:top=side(1) style:width=side(2) style:height=side(3)>
                <Part part=image_cropper::Part::Selection label="Zoomed crop" />
            </div>
        </Part>
    }
}

#[component]
fn TimeParts() -> impl IntoView {
    let input = use_primitive::<DateInput>();
    view! {
        <Part part=date_input::Part::Root>
            <div class="segments">
                <For each=move || input.with(|i| i.segments()) key=|segment| segment.name() let:segment>
                    <Part part=date_input::Part::Segment item=segment.name() label=segment.name() />
                </For>
            </div>
        </Part>
    }
}

#[component]
fn CityField() -> impl IntoView {
    let combobox = use_primitive::<Combobox>();
    view! {
        <Part part=combobox::Part::Input />
        <span id="city-completion" class="hint">{move || combobox.with(|c| c.completion()).unwrap_or_default()}</span>
        <Part part=combobox::Part::List>
            <Part part=combobox::Part::Option item="lisbon">"Lisbon"</Part>
            <Part part=combobox::Part::Option item="lima">"Lima"</Part>
            <Part part=combobox::Part::Option item="oslo">"Oslo"</Part>
        </Part>
    }
}

#[component]
fn CommandField() -> impl IntoView {
    let menu = use_primitive::<TriggerMenu>();
    view! {
        <Part part=trigger_menu::Part::Input />
        <span id="command-hint" class="hint">{move || menu.with(|m| m.argument_hint().map(str::to_string)).unwrap_or_default()}</span>
        <Part part=trigger_menu::Part::List>
            <Part part=trigger_menu::Part::Option item="/deploy">"deploy"</Part>
            <Part part=trigger_menu::Part::Option item="/rollback">"rollback"</Part>
        </Part>
    }
}

#[component]
pub fn Audit() -> impl IntoView {
    let (price, on_price) = follow();
    let (panes, on_panes) = follow();
    let (stars, on_stars) = follow();
    let (stay, on_stay) = follow();
    let (sizes, on_sizes) = follow();
    let (palette, on_palette) = follow();
    let (amount, on_amount) = follow();
    let (time, on_time) = follow();
    let (city, on_city) = follow();
    let (line, on_line) = follow();
    let (zoomed, on_zoomed) = follow();
    let searches = RwSignal::new(Vec::<String>::new());
    let answers = RwSignal::new(Vec::<(String, String)>::new());
    let step = RwSignal::new(vec![("current".to_string(), "packed".to_string())]);
    // The children of a folder arrive when it is first opened.
    let loaded = RwSignal::new(Vec::<(String, String)>::new());
    let asked = RwSignal::new(String::new());
    let on_remote = Callback::new(move |change: ValueChange| {
        if let Some(key) = change
            .value
            .first()
            .and_then(|entry| entry.strip_prefix("load:"))
        {
            asked.set(key.to_string());
            loaded.set(vec![(
                "children".to_string(),
                format!("{key}={key}/a.txt,{key}/b.txt"),
            )]);
        }
        true
    });

    view! {
        <Section title="Event list">
            <Root machine=EventList::default() label="Order progress" settings=step>
                <Part part=event_list::Part::Root>
                    <Part part=event_list::Part::Event item="ordered">"Ordered"</Part>
                    <Part part=event_list::Part::Event item="packed">"Packed"</Part>
                    <Part part=event_list::Part::Event item="shipped">"Shipped"</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Range slider">
            <p id="price-value">"Price: " {move || price.get().join(" to ")}</p>
            <Root machine=RangeSlider::default() label="Price" on_value_change=on_price><Grips /></Root>
        </Section>

        <Section title="Panel group">
            <p id="panes-value">"Panes: " {move || panes.get().join(" / ")}</p>
            <Root machine=PanelGroup::default() label="Layout" on_value_change=on_panes><Panes /></Root>
        </Section>

        <Section title="Half-star rating">
            <p id="stars-value">"Stars: " {move || stars.get().join("")}</p>
            <Root machine=Rating::new(rating::Options { halves: true, ..Default::default() }) label="Review" on_value_change=on_stars><Stars /></Root>
        </Section>

        <Section title="Date span">
            <p id="stay-value">"Stay: " {move || stay.get().join(" to ")}</p>
            <Root
                machine=Calendar::new(calendar::Options { mode: Selecting::Range, months: 2, ..Default::default() }, Date::new(2026, 11, 17).expect("a real date"))
                label="Stay"
                on_value_change=on_stay
            >
                <TwoMonths />
            </Root>
        </Section>

        <Section title="Multiple select">
            <p id="sizes-value">"Sizes: " {move || sizes.get().join(", ")}</p>
            <Root machine=Select::multiple() label="Sizes in stock" on_value_change=on_sizes>
                <Part part=select::Part::Trigger>{move || if sizes.get().is_empty() { "Choose sizes".to_string() } else { sizes.get().join(", ") }}</Part>
                <Part part=select::Part::Content>
                    <Part part=select::Part::Option item="small">"Small"</Part>
                    <Part part=select::Part::Option item="medium">"Medium"</Part>
                    <Part part=select::Part::Option item="large">"Large"</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Tree that loads">
            <p id="remote-value">"Asked for: " {move || asked.get()}</p>
            <Root machine=TreeView::new(tree_from_paths(&["remote", "notes.md"]), Dir::Ltr).lazy(&["remote"]) label="Remote files" on_value_change=on_remote settings=loaded>
                <LazyRows />
            </Root>
        </Section>

        <Section title="Command pages">
            <p id="palette-value">"Ran: " {move || palette.get().join("")}</p>
            <Root machine=Command::default().page("theme", "change-theme", &["light", "dark"]) label="Palette" on_value_change=on_palette>
                <Part part=command::Part::Input />
                <Part part=command::Part::List>
                    <Part part=command::Part::Option item="save">"Save file"</Part>
                    <Part part=command::Part::Option item="change-theme">"Change theme…"</Part>
                    <Part part=command::Part::Option item="light">"Light"</Part>
                    <Part part=command::Part::Option item="dark">"Dark"</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Number scrub and wheel">
            <p id="amount-value">"Amount: " {move || amount.get().join("")}</p>
            <Root machine=NumberInput::new(NumberRange::new(0.0, 100.0, 1.0), 20.0) label="Amount" on_value_change=on_amount>
                <Part part=number_input::Part::Scrub><span class="scrub">"⇔ Amount"</span></Part>
                <Part part=number_input::Part::Input />
            </Root>
        </Section>

        <Section title="Twelve-hour time">
            <p id="time-value">"Time: " {move || time.get().join("")}</p>
            <Root machine=DateInput::new(true).twelve_hour() label="Meeting" on_value_change=on_time><TimeParts /></Root>
        </Section>

        <Section title="Completion and more">
            <p id="city-value">"City: " {move || city.get().join(", ")}</p>
            <Root machine=Combobox::new(combobox::Options { load_more: true, ..Default::default() }) label="City" on_value_change=on_city><CityField /></Root>
        </Section>

        <Section title="Remote search">
            <p id="remote-search-value">"Asked: " {move || searches.get().join(" ")}</p>
            <Root
                machine=Combobox::new(combobox::Options { remote: true, ..Default::default() })
                label="Remote search"
                settings=answers
                on_value_change=Callback::new(move |change: ValueChange| {
                    // Each search is numbered. Only the first is answered here, late, to
                    // show that an answer to a search since replaced is ignored.
                    if let Some(asked) = change.value.first().and_then(|entry| entry.strip_prefix("load:")) {
                        searches.update(|list| list.push(asked.to_string()));
                        if asked.starts_with("1:") {
                            answers.set(vec![("loaded".to_string(), "1".to_string())]);
                        }
                    }
                    true
                })
            >
                <Part part=combobox::Part::Input />
                <Part part=combobox::Part::List>
                    <Part part=combobox::Part::Option item="lisbon">"Lisbon"</Part>
                    <Part part=combobox::Part::Status>"Searching…"</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Command arguments">
            <p id="line-value">"Line: " {move || line.get().join("")}</p>
            <Root machine=TriggerMenu::new(&['/']).arguments("/deploy", &["environment", "version"]) label="Command line" on_value_change=on_line><CommandField /></Root>
        </Section>

        <Section title="Zoomed crop">
            <p id="zoomed-value">"Crop: " {move || zoomed.get().join("")}</p>
            <Root machine=ImageCropper::new(Size { width: 800.0, height: 600.0 }, Some(1.0)) label="Zoomed crop" on_value_change=on_zoomed><ZoomCrop /></Root>
        </Section>
    }
}
