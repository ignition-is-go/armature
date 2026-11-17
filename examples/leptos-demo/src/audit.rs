//! The components and options added after comparing every option with UI SDK's.

use armature_core::calendar::{self, Calendar, Selecting};
use armature_core::command::{self, Command};
use armature_core::date::Date;
use armature_core::event_list::{self, EventList};
use armature_core::keys::Dir;
use armature_core::panel_group::{self, PanelGroup};
use armature_core::range_slider::{self, RangeSlider};
use armature_core::rating::{self, Rating};
use armature_core::select::{self, Select};
use armature_core::tree::tree_from_paths;
use armature_core::tree_view::{self, TreeView};
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
pub fn Audit() -> impl IntoView {
    let (price, on_price) = follow();
    let (panes, on_panes) = follow();
    let (stars, on_stars) = follow();
    let (stay, on_stay) = follow();
    let (sizes, on_sizes) = follow();
    let (palette, on_palette) = follow();
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
    }
}
