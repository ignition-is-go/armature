use armature_core::accordion::{self, Accordion};
use armature_core::calendar::{self, Calendar};
use armature_core::command::{self, Command};
use armature_core::date::Date;
use armature_core::dialog::{self, Dialog};
use armature_core::keys::Dir;
use armature_core::listbox::{self, Listbox};
use armature_core::number::NumberRange;
use armature_core::number_input::{self, NumberInput};
use armature_core::otp_input::{self, OtpInput};
use armature_core::radio::{self, Radio};
use armature_core::resizable::{self, Resizable};
use armature_core::select::{self, Select};
use armature_core::selection::Mode;
use armature_core::slider::{self, Slider};
use armature_core::sortable::{self, Sortable};
use armature_core::tabs::{self, Tabs};
use armature_core::toggle::{self, Toggle};
use armature_core::token_input::{self, TokenInput};
use armature_core::toolbar::{self, Toolbar};
use armature_core::tree::tree_from_paths;
use armature_core::tree_view::{self, TreeView};
use armature_leptos::{Part, Root, Source, ValueChange, agent, use_primitive};
use leptos::prelude::*;

/// A signal that follows a component's value, and the gate that feeds it.
fn follow() -> (RwSignal<Vec<String>>, Callback<ValueChange, bool>) {
    let value = RwSignal::new(Vec::<String>::new());
    let gate = Callback::new(move |change: ValueChange| {
        value.set(change.value);
        true
    });
    (value, gate)
}

#[component]
fn Section(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section>
            <h3>{title}</h3>
            {children()}
        </section>
    }
}

/// The parts of a calendar, drawn from the calendar's own weeks.
#[component]
fn CalendarParts() -> impl IntoView {
    let calendar = use_primitive::<Calendar>();
    let days = move || calendar.with(|c| c.weeks().into_iter().flatten().collect::<Vec<Date>>());
    let title = move || calendar.with(|c| format!("{}-{:02}", c.month().0, c.month().1));
    view! {
        <div class="calendar-head">
            <Part part=calendar::Part::PreviousMonth label="Previous month">"‹"</Part>
            <span id="calendar-month">{title}</span>
            <Part part=calendar::Part::NextMonth label="Next month">"›"</Part>
        </div>
        <Part part=calendar::Part::Grid>
            <For each=days key=|day| *day let:day>
                <Part part=calendar::Part::Day item=day.format()>{day.day}</Part>
            </For>
        </Part>
    }
}

/// The cells of a one-time code, drawn over its text field.
#[component]
fn OtpCells() -> impl IntoView {
    let otp = use_primitive::<OtpInput>();
    view! {
        <div class="otp-cells">
            {move || {
                otp.with(|o| o.cells())
                    .into_iter()
                    .map(|digit| view! { <span>{digit.map(String::from).unwrap_or_default()}</span> })
                    .collect_view()
            }}
        </div>
    }
}

/// The two panels of a resizable box, sized by its divider.
#[component]
fn Panels() -> impl IntoView {
    let panels = use_primitive::<Resizable>();
    let share = move || format!("{}%", panels.with(|p| p.percent()));
    view! {
        <div class="panel" style:width=share>"Left"</div>
        <Part part=resizable::Part::Handle />
        <div class="panel grow">"Right"</div>
    }
}

#[component]
fn App() -> impl IntoView {
    let (size, on_size) = follow();
    let (fruit, on_fruit) = follow();
    let (volume, on_volume) = follow();
    let (wifi, on_wifi) = follow();
    let (faq, on_faq) = follow();
    let (rating, on_rating) = follow();
    let (file, on_file) = follow();
    let (ran, on_ran) = follow();
    let (quantity, on_quantity) = follow();
    let (groceries, on_groceries) = follow();
    let (day, on_day) = follow();
    let (pressed, on_pressed) = follow();
    let (code, on_code) = follow();
    let (tags, on_tags) = follow();
    let (split, on_split) = follow();
    let start = Date::parse("2026-11-17").expect("a real date");

    let access = RwSignal::new(Vec::<String>::new());
    let on_access = Callback::new(move |change: ValueChange| {
        let agent_grants_delete =
            change.source == Source::Agent && change.value.iter().any(|v| v == "delete");
        if !agent_grants_delete {
            access.set(change.value);
        }
        !agent_grants_delete
    });
    let multiple = listbox::Options {
        mode: Mode::Multiple,
        ..Default::default()
    };
    let percent = move || format!("{}%", volume.get().first().cloned().unwrap_or("30".into()));
    let files = [
        "src",
        "src/main.rs",
        "src/ui",
        "src/ui/button.rs",
        "docs",
        "docs/intro.md",
        "README.md",
    ];
    let tree = tree_from_paths(&[
        "src/main.rs",
        "src/ui/button.rs",
        "docs/intro.md",
        "README.md",
    ]);
    let shopping = RwSignal::new(vec!["milk", "eggs", "bread", "jam"]);
    Effect::new(move |_| {
        let order = groceries.get();
        if !order.is_empty() {
            shopping.update(|list| list.sort_by_key(|key| order.iter().position(|k| k == key)));
        }
    });

    view! {
        <main>
            <Section title="Select">
                <Root machine=Select::default() label="Size" on_value_change=on_size>
                    <Part part=select::Part::Trigger>
                        <span id="size-value">
                            {move || size.get().first().cloned().unwrap_or("Choose a size".into())}
                        </span>
                    </Part>
                    <Part part=select::Part::Content>
                        <Part part=select::Part::Option item="small">"Small"</Part>
                        <Part part=select::Part::Option item="medium" disabled=true>"Medium"</Part>
                        <Part part=select::Part::Option item="large">"Large"</Part>
                    </Part>
                </Root>
                <button id="elsewhere">"Elsewhere"</button>
            </Section>

            <Section title="Tabs and listbox">
                <Root machine=Tabs::default() label="Settings">
                    <Part part=tabs::Part::List>
                        <Part part=tabs::Part::Tab item="fruit">"Fruit"</Part>
                        <Part part=tabs::Part::Tab item="billing" disabled=true>"Billing"</Part>
                        <Part part=tabs::Part::Tab item="access">"Access"</Part>
                    </Part>
                    <Part part=tabs::Part::Panel item="fruit">
                        <p id="fruit-value">"Fruit: " {move || fruit.get().join(", ")}</p>
                        <Root machine=Listbox::default() label="Fruit" on_value_change=on_fruit>
                            <Part part=listbox::Part::Root>
                                <Part part=listbox::Part::Option item="apple">"Apple"</Part>
                                <Part part=listbox::Part::Option item="banana" disabled=true>
                                    "Banana"
                                </Part>
                                <Part part=listbox::Part::Option item="blueberry">"Blueberry"</Part>
                                <Part part=listbox::Part::Option item="cherry">"Cherry"</Part>
                            </Part>
                        </Root>
                    </Part>
                    <Part part=tabs::Part::Panel item="access">
                        <p id="access-value">"Access: " {move || access.get().join(", ")}</p>
                        <p>"Agents may not grant delete."</p>
                        <Root machine=Listbox::new(multiple) label="Access" on_value_change=on_access>
                            <Part part=listbox::Part::Root>
                                <Part part=listbox::Part::Option item="read">"Read"</Part>
                                <Part part=listbox::Part::Option item="write">"Write"</Part>
                                <Part part=listbox::Part::Option item="share">"Share"</Part>
                                <Part part=listbox::Part::Option item="delete">"Delete"</Part>
                            </Part>
                        </Root>
                    </Part>
                </Root>
            </Section>

            <Section title="Slider">
                <p id="volume-value">"Volume: " {percent}</p>
                <Root machine=Slider::new(Default::default(), 30.0) label="Volume" on_value_change=on_volume>
                    <Part part=slider::Part::Track>
                        <div class="fill" style:width=percent></div>
                        <div class="thumb-slot" style:left=percent>
                            <Part part=slider::Part::Thumb />
                        </div>
                    </Part>
                </Root>
            </Section>

            <Section title="Switch">
                <Root machine=Toggle::default() label="Wi-Fi" on_value_change=on_wifi>
                    <Part part=toggle::Part::Root>
                        <span class="knob"></span>
                    </Part>
                </Root>
                <span id="wifi-value">{move || if wifi.get().is_empty() { "Off" } else { "On" }}</span>
            </Section>

            <Section title="Accordion">
                <p id="faq-value">"Open: " {move || faq.get().join(", ")}</p>
                <Root machine=Accordion::default() label="Questions" on_value_change=on_faq>
                    <Part part=accordion::Part::Trigger item="shipping">"Shipping"</Part>
                    <Part part=accordion::Part::Panel item="shipping">"Ships in two days."</Part>
                    <Part part=accordion::Part::Trigger item="returns">"Returns"</Part>
                    <Part part=accordion::Part::Panel item="returns">"Thirty days."</Part>
                </Root>
            </Section>

            <Section title="Rating">
                <p id="rating-value">"Rating: " {move || rating.get().join("")}</p>
                <Root machine=Radio::new(Dir::Ltr) label="Rating" on_value_change=on_rating>
                    <Part part=radio::Part::Root>
                        <Part part=radio::Part::Option item="1" label="1 star">"★"</Part>
                        <Part part=radio::Part::Option item="2" label="2 stars">"★"</Part>
                        <Part part=radio::Part::Option item="3" label="3 stars">"★"</Part>
                        <Part part=radio::Part::Option item="4" label="4 stars">"★"</Part>
                    </Part>
                </Root>
            </Section>

            <Section title="Tree">
                <p id="file-value">"File: " {move || file.get().join("")}</p>
                <Root machine=TreeView::new(tree, Dir::Ltr) label="Files" on_value_change=on_file>
                    <Part part=tree_view::Part::Root>
                        {files
                            .map(|path| {
                                let name = path.rsplit('/').next().unwrap_or(path);
                                view! { <Part part=tree_view::Part::Node item=path>{name}</Part> }
                            })
                            .collect_view()}
                    </Part>
                </Root>
            </Section>

            <Section title="Command">
                <p id="command-value">"Ran: " {move || ran.get().join("")}</p>
                <Root machine=Command::default() label="Commands" on_value_change=on_ran>
                    <Part part=command::Part::Input />
                    <Part part=command::Part::List>
                        <Part part=command::Part::Option item="open">"Open file"</Part>
                        <Part part=command::Part::Option item="save">"Save file"</Part>
                        <Part part=command::Part::Option item="close">"Close window"</Part>
                    </Part>
                </Root>
            </Section>

            <Section title="Number">
                <p id="quantity-value">"Quantity: " {move || quantity.get().join("")}</p>
                <Root
                    machine=NumberInput::new(NumberRange::new(0.0, 20.0, 1.0), 2.0)
                    label="Quantity"
                    on_value_change=on_quantity
                >
                    <div class="stepper">
                        <Part part=number_input::Part::Decrement label="Decrease">"−"</Part>
                        <Part part=number_input::Part::Input />
                        <Part part=number_input::Part::Increment label="Increase">"+"</Part>
                    </div>
                </Root>
            </Section>

            <Section title="Sortable">
                <p id="groceries-value">"Order: " {move || groceries.get().join(", ")}</p>
                <Root machine=Sortable::default() label="Groceries" on_value_change=on_groceries>
                    <Part part=sortable::Part::Root>
                        <For each=move || shopping.get() key=|key| *key let:key>
                            <Part part=sortable::Part::Item item=key>{key}</Part>
                        </For>
                    </Part>
                </Root>
            </Section>

            <Section title="Calendar">
                <p id="day-value">"Day: " {move || day.get().join("")}</p>
                <Root machine=Calendar::new(Default::default(), start) label="Date" on_value_change=on_day>
                    <CalendarParts />
                </Root>
            </Section>

            <Section title="Toolbar">
                <p id="pressed-value">"Pressed: " {move || pressed.get().join("")}</p>
                <Root machine=Toolbar::default() label="Formatting" on_value_change=on_pressed>
                    <Part part=toolbar::Part::Root>
                        <Part part=toolbar::Part::Button item="bold">"Bold"</Part>
                        <Part part=toolbar::Part::Button item="italic" disabled=true>"Italic"</Part>
                        <Part part=toolbar::Part::Button item="link">"Link"</Part>
                    </Part>
                </Root>
            </Section>

            <Section title="One-time code">
                <p id="code-value">"Code: " {move || code.get().join("")}</p>
                <Root machine=OtpInput::new(4) label="Code" on_value_change=on_code>
                    <div class="otp">
                        <Part part=otp_input::Part::Input />
                        <OtpCells />
                    </div>
                </Root>
            </Section>

            <Section title="Tokens">
                <p id="tags-value">"Tags: " {move || tags.get().join(", ")}</p>
                <Root machine=TokenInput::default() label="Tags" on_value_change=on_tags>
                    <Part part=token_input::Part::Root>
                        <For each=move || tags.get() key=|tag| tag.clone() let:tag>
                            <Part part=token_input::Part::Token item=tag.clone() label=format!("Remove {tag}")>
                                {tag.clone()}
                                " ×"
                            </Part>
                        </For>
                        <Part part=token_input::Part::Input />
                    </Part>
                </Root>
            </Section>

            <Section title="Resizable">
                <p id="split-value">"Split: " {move || split.get().join("")}</p>
                <Root machine=Resizable::default() label="Panels" on_value_change=on_split>
                    <Part part=resizable::Part::Root>
                        <Panels />
                    </Part>
                </Root>
            </Section>

            <Section title="Dialog">
                <Root machine=Dialog::default() label="Confirm">
                    <Part part=dialog::Part::Trigger>"Open dialog"</Part>
                    <Part part=dialog::Part::Backdrop />
                    <Part part=dialog::Part::Content>
                        <p>"Are you sure?"</p>
                        <button id="dialog-yes">"Yes"</button>
                        <Part part=dialog::Part::Close>"Cancel"</Part>
                    </Part>
                </Root>
            </Section>

            <Section title="Agent rules">
                <Root machine=Toggle::default() label="Delete account" agent_confirm="Delete the account">
                    <Part part=toggle::Part::Root>
                        <span class="knob"></span>
                    </Part>
                </Root>
                <Root machine=Toggle::default() label="Admin mode" agent_hidden=true>
                    <Part part=toggle::Part::Root>
                        <span class="knob"></span>
                    </Part>
                </Root>
                <Root
                    machine=NumberInput::new(NumberRange::new(0.0, 9999.0, 1.0), 1234.0)
                    label="PIN"
                    agent_secret=true
                >
                    <Part part=number_input::Part::Input />
                </Root>
            </Section>
        </main>
    }
}

fn main() {
    agent::install();
    agent::register_tools();
    mount_to_body(App);
}
