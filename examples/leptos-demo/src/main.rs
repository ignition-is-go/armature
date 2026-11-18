mod audit;
mod more;

use armature_core::accordion::{self, Accordion};
use armature_core::calendar::{self, Calendar};
use armature_core::canvas::{self, Canvas, Node};
use armature_core::color_picker::{self, ColorPicker};
use armature_core::command::{self, Command};
use armature_core::data_grid::{self, DataGrid};
use armature_core::date::Date;
use armature_core::dialog::{self, Dialog};
use armature_core::editable::{self, Editable};
use armature_core::geometry::Rect;
use armature_core::hold_button::{self, HoldButton};
use armature_core::keys::Dir;
use armature_core::listbox::{self, Listbox};
use armature_core::number::NumberRange;
use armature_core::number_input::{self, NumberInput};
use armature_core::otp_input::{self, OtpInput};
use armature_core::picker::{self, Picker};
use armature_core::popover::{self, Popover};
use armature_core::preference::{ColorScheme, Preference};
use armature_core::prompt::{self, Prompt};
use armature_core::radio::{self, Radio};
use armature_core::resizable::{self, Resizable};
use armature_core::select::{self, Select};
use armature_core::selection::Mode;
use armature_core::sheet::{self, Sheet};
use armature_core::slider::{self, Slider};
use armature_core::sortable::{self, Sortable};
use armature_core::swipe_row::{self, SwipeRow};
use armature_core::tabs::{self, Tabs};
use armature_core::timeline::{self, Timeline};
use armature_core::toggle::{self, Toggle};
use armature_core::token_input::{self, TokenInput};
use armature_core::toolbar::{self, Toolbar};
use armature_core::tooltip::{self, Tooltip};
use armature_core::transfer_list::{self, TransferList};
use armature_core::tree::tree_from_paths;
use armature_core::tree_view::{self, TreeView};
use armature_core::virtual_list::{self, VirtualList};
use armature_leptos::{
    CookieStorage, LocalStorage, Part, Root, Source, ValueChange, agent, system_prefers_dark,
    use_primitive, use_shortcut, use_visible_viewport,
};
use leptos::prelude::*;

/// A signal that follows a component's value, and the gate that feeds it.
pub(crate) fn follow() -> (RwSignal<Vec<String>>, Callback<ValueChange, bool>) {
    let value = RwSignal::new(Vec::<String>::new());
    let gate = Callback::new(move |change: ValueChange| {
        value.set(change.value);
        true
    });
    (value, gate)
}

#[allow(
    clippy::must_use_candidate,
    reason = "the component macro generates a function this cannot reach"
)]
#[component]
pub(crate) fn Section(title: &'static str, children: Children) -> impl IntoView {
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
                otp.with(armature_core::otp_input::OtpInput::cells)
                    .into_iter()
                    .map(|digit| view! { <span>{digit.map(String::from).unwrap_or_default()}</span> })
                    .collect_view()
            }}
        </div>
    }
}

/// A colour picker's square, strip and hex field, painted from its colour.
#[component]
fn ColorParts() -> impl IntoView {
    let picker = use_primitive::<ColorPicker>();
    let percent = |fraction: f64| format!("{}%", fraction * 100.0);
    view! {
        <Part part=color_picker::Part::Area>
            <div class="color-pad" style:background-color=move || picker.with(|p| p.hue_color().to_hex())></div>
            <div
                class="color-dot"
                style:left=move || percent(picker.with(|p| p.area_position().0))
                style:top=move || percent(picker.with(|p| p.area_position().1))
            >
                <Part part=color_picker::Part::AreaThumb label="Saturation and brightness" />
            </div>
        </Part>
        <Part part=color_picker::Part::Hue>
            <div class="color-dot" style:left=move || percent(picker.with(armature_core::color_picker::ColorPicker::hue_position)) style:top="50%">
                <Part part=color_picker::Part::HueThumb label="Hue" />
            </div>
        </Part>
        <div class="color-row">
            <span id="swatch" class="swatch" style:background-color=move || picker.with(|p| p.color().to_hex())></span>
            <Part part=color_picker::Part::Input />
        </div>
    }
}

/// A sheet sized by its own height.
#[component]
fn SheetParts() -> impl IntoView {
    let panel = use_primitive::<Sheet>();
    view! {
        <Part part=sheet::Part::Trigger>"Open sheet"</Part>
        <Part part=sheet::Part::Backdrop />
        <div class="sheet-slot" style:height=move || format!("{}px", panel.with(armature_core::sheet::Sheet::height))>
            <Part part=sheet::Part::Content>
                <Part part=sheet::Part::Handle label="Resize sheet" />
                <p>"Drag the grip, or use the arrow keys."</p>
            </Part>
        </div>
    }
}

/// A row that slides aside to show its actions.
#[component]
fn SwipeParts() -> impl IntoView {
    let row = use_primitive::<SwipeRow>();
    view! {
        <div class="swipe">
            <div class="swipe-actions">
                <Part part=swipe_row::Part::Action item="archive">"Archive"</Part>
                <Part part=swipe_row::Part::Action item="delete">"Delete"</Part>
            </div>
            <div class="swipe-slide" style:transform=move || format!("translateX(-{}px)", row.with(armature_core::swipe_row::SwipeRow::offset))>
                <Part part=swipe_row::Part::Row>"Swipe me left"</Part>
            </div>
        </div>
    }
}

/// A long list that renders only the rows in view.
#[component]
fn VirtualRows() -> impl IntoView {
    let list = use_primitive::<VirtualList>();
    view! {
        <Part part=virtual_list::Part::Viewport>
            <div class="virtual-space" style:height=move || format!("{}px", list.with(armature_core::virtual_list::VirtualList::total_size))>
                <For each=move || list.with(armature_core::virtual_list::VirtualList::rows) key=|row| row.index let:row>
                    <div class="virtual-row" style:top=format!("{}px", row.offset)>
                        <Part part=virtual_list::Part::Row item=row.key.clone()>
                            "Row "
                            {row.index}
                        </Part>
                    </div>
                </For>
            </div>
        </Part>
    }
}

/// A day's track with spans drawn, moved and resized on it.
#[component]
fn TimelineParts() -> impl IntoView {
    let timeline = use_primitive::<Timeline>();
    let spans = move || {
        timeline.with(|t| {
            let lanes = t.lanes();
            t.spans()
                .iter()
                .zip(lanes)
                .map(|((key, span), lane)| (key.clone(), t.position_of(*span), lane))
                .collect::<Vec<_>>()
        })
    };
    view! {
        <Part part=timeline::Part::Track>
            <For
                each=spans
                key=|(key, (start, end), lane)| (key.clone(), start.to_bits(), end.to_bits(), *lane)
                let((key, (start, end), lane))
            >
                <div
                    class="span-slot"
                    style:left=format!("{}%", start * 100.0)
                    style:width=format!("{}%", (end - start) * 100.0)
                    style:top=format!("{}px", lane.saturating_mul(22))
                >
                    <Part part=timeline::Part::Span item=key.clone()>
                        {key.clone()}
                        <Part part=timeline::Part::SpanEnd item=key.clone() />
                    </Part>
                </div>
            </For>
        </Part>
    }
}

/// A grid of cells showing its own text.
#[component]
fn GridCells() -> impl IntoView {
    let grid = use_primitive::<DataGrid>();
    let cells =
        (0..3).flat_map(|row| (0..3).map(move |col| armature_core::grid::Cell { row, col }));
    view! {
        <Part part=data_grid::Part::Root>
            {cells
                .map(|cell| {
                    view! {
                        <Part part=data_grid::Part::Cell item=format!("{},{}", cell.row, cell.col)>
                            {move || grid.with(|g| g.text_of(cell).to_string())}
                        </Part>
                    }
                })
                .collect_view()}
        </Part>
    }
}

/// Nodes on a surface, placed from the canvas's own view of them.
#[component]
fn CanvasParts() -> impl IntoView {
    let canvas = use_primitive::<Canvas>();
    let place = move |key: &'static str| {
        move || {
            let rect = canvas.with(|c| c.screen_rect(key)).unwrap_or_default();
            format!(
                "left:{}px;top:{}px;width:{}px;height:{}px",
                rect.x, rect.y, rect.width, rect.height
            )
        }
    };
    let lasso = move || {
        canvas
            .with(armature_core::canvas::Canvas::lasso)
            .map(|rect| {
                format!(
                    "left:{}px;top:{}px;width:{}px;height:{}px",
                    rect.x, rect.y, rect.width, rect.height
                )
            })
    };
    view! {
        <Part part=canvas::Part::Surface>
            {["a", "b", "c"]
                .map(|key| {
                    view! {
                        <div class="node-slot" style=place(key)>
                            <Part part=canvas::Part::Node item=key>
                                {key}
                                <Part part=canvas::Part::PortIn item=key label=format!("Into {key}")>"●"</Part>
                                <Part part=canvas::Part::PortOut item=key label=format!("Out of {key}")>"●"</Part>
                                <Part part=canvas::Part::Resize item=key />
                            </Part>
                        </div>
                    }
                })
                .collect_view()}
            <div class="lasso" style=move || lasso().unwrap_or_else(|| "display:none".into())></div>
        </Part>
        <p id="links">{move || canvas.with(|c| c.connections().iter().map(|l| format!("{}>{}", l.from, l.to)).collect::<Vec<_>>().join(" "))}</p>
        <p id="canvas-selected">{move || canvas.with(|c| c.selected().join(","))}</p>
    }
}

/// The two panels of a resizable box, sized by its divider.
#[component]
fn Panels() -> impl IntoView {
    let panels = use_primitive::<Resizable>();
    let share = move || {
        format!(
            "{}%",
            panels.with(armature_core::resizable::Resizable::percent)
        )
    };
    view! {
        <div class="panel" style:width=share>"Left"</div>
        <Part part=resizable::Part::Handle />
        <div class="panel grow">"Right"</div>
    }
}

/// A tree drawn from its own rows, so a node that is moved is drawn where it lands.
#[component]
fn OutlineRows() -> impl IntoView {
    let tree = use_primitive::<TreeView>();
    let level = move |key: &str| {
        tree.with(|t| {
            t.rows()
                .iter()
                .find(|row| row.key == key)
                .map_or(1, |row| row.level)
        })
    };
    view! {
        <Part part=tree_view::Part::Root>
            <For each=move || tree.with(armature_core::tree_view::TreeView::rows) key=|row| row.key.clone() let:row>
                {
                    let key = row.key.clone();
                    let name = row.key.rsplit('/').next().unwrap_or(&row.key).to_string();
                    view! {
                        <div class="outline-row" style:padding-left=move || format!("{}px", level(&key).saturating_sub(1).saturating_mul(24))>
                            <Part part=tree_view::Part::Node item=row.key>{name}</Part>
                        </div>
                    }
                }
            </For>
        </Part>
    }
}

/// Two lists drawn from the primitive, so an item is drawn in the list that holds it.
#[component]
fn TransferParts() -> impl IntoView {
    let lists = use_primitive::<TransferList>();
    let column = move |chosen: bool| {
        view! {
            <For each=move || lists.with(|l| l.list(chosen).to_vec()) key=|key| key.clone() let:key>
                <Part part=transfer_list::Part::Option item=key.clone()>{key.clone()}</Part>
            </For>
        }
    };
    view! {
        <div class="transfer">
            <Part part=transfer_list::Part::Available label="Available">{column(false)}</Part>
            <div class="transfer-buttons">
                <Part part=transfer_list::Part::Add label="Add">"→"</Part>
                <Part part=transfer_list::Part::Remove label="Remove">"←"</Part>
            </div>
            <Part part=transfer_list::Part::Chosen label="Chosen">{column(true)}</Part>
        </div>
    }
}

/// A colour scheme choice that is remembered, and applied before the page paints by
/// the script in `index.html`.
#[component]
fn ThemeChoice() -> impl IntoView {
    const KEY: &str = "armature-theme";
    let stored = Preference::load(KEY, &ColorScheme::OPTIONS, "system", &LocalStorage);
    let theme = RwSignal::new(vec![stored.value().to_string()]);
    let on_theme = Callback::new(move |change: ValueChange| {
        let Some(choice) = change.value.first() else {
            return false;
        };
        let mut preference = Preference::load(KEY, &ColorScheme::OPTIONS, "system", &LocalStorage);
        if !preference.choose(choice, &mut LocalStorage) {
            return false;
        }
        // The cookie lets a server render the right scheme on the next visit.
        armature_core::preference::Storage::set(&mut CookieStorage, KEY, choice);
        let dark = system_prefers_dark();
        let scheme = ColorScheme::parse(choice).unwrap_or(ColorScheme::System);
        let shown = if scheme.is_dark(dark) {
            "dark"
        } else {
            "light"
        };
        if let Some(root) = document().document_element() {
            let _ = root.set_attribute("data-theme", shown);
        }
        theme.set(change.value);
        true
    });
    view! {
        <p id="theme-value">"Theme: " {move || theme.get().join("")}</p>
        <Root machine=Radio::new(Dir::Ltr) label="Theme" on_value_change=on_theme value=theme>
            <Part part=radio::Part::Root>
                <Part part=radio::Part::Option item="light">"Light"</Part>
                <Part part=radio::Part::Option item="dark">"Dark"</Part>
                <Part part=radio::Part::Option item="system">"System"</Part>
            </Part>
        </Root>
    }
}

#[component]
fn SelectSection() -> impl IntoView {
    let (size, on_size) = follow();
    view! {
        <Section title="Select">
            <Root machine=Select::default() label="Size" on_value_change=on_size>
                <Part part=select::Part::Trigger>
                    <span id="size-value">
                        {move || size.get().first().cloned().unwrap_or_else(|| "Choose a size".into())}
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
    }
}

#[component]
fn TabsAndListboxSection() -> impl IntoView {
    let (fruit, on_fruit) = follow();
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
    view! {
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
    }
}

#[component]
fn SliderSection() -> impl IntoView {
    let (volume, on_volume) = follow();
    let percent = move || {
        format!(
            "{}%",
            volume.get().first().cloned().unwrap_or_else(|| "30".into())
        )
    };
    view! {
        <Section title="Slider">
            <p id="volume-value">"Volume: " {percent}</p>
            <Root machine=Slider::new(armature_core::slider::Options::default(), 30.0) label="Volume" on_value_change=on_volume>
                <Part part=slider::Part::Track>
                    <div class="fill" style:width=percent></div>
                    <div class="thumb-slot" style:left=percent>
                        <Part part=slider::Part::Thumb />
                    </div>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn SwitchSection() -> impl IntoView {
    let (wifi, on_wifi) = follow();
    view! {
        <Section title="Switch">
            <Root machine=Toggle::default() label="Wi-Fi" on_value_change=on_wifi>
                <Part part=toggle::Part::Root>
                    <span class="knob"></span>
                </Part>
            </Root>
            <span id="wifi-value">{move || if wifi.get().is_empty() { "Off" } else { "On" }}</span>
        </Section>
    }
}

#[component]
fn AccordionSection() -> impl IntoView {
    let (faq, on_faq) = follow();
    view! {
        <Section title="Accordion">
            <p id="faq-value">"Open: " {move || faq.get().join(", ")}</p>
            <Root machine=Accordion::default() label="Questions" on_value_change=on_faq>
                <Part part=accordion::Part::Trigger item="shipping">"Shipping"</Part>
                <Part part=accordion::Part::Panel item="shipping">"Ships in two days."</Part>
                <Part part=accordion::Part::Trigger item="returns">"Returns"</Part>
                <Part part=accordion::Part::Panel item="returns">"Thirty days."</Part>
            </Root>
        </Section>
    }
}

#[component]
fn RatingSection() -> impl IntoView {
    let (rating, on_rating) = follow();
    view! {
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
    }
}

#[component]
fn TreeSection() -> impl IntoView {
    let (file, on_file) = follow();
    let tree = tree_from_paths(&[
        "src/main.rs",
        "src/ui/button.rs",
        "docs/intro.md",
        "README.md",
    ]);
    // Every folder and file, in the order the tree lists them.
    let nodes = armature_core::tree::Tree::new(&tree).subtree_order();
    view! {
        <Section title="Tree">
            <p id="file-value">"File: " {move || file.get().join("")}</p>
            <Root machine=TreeView::new(tree, Dir::Ltr) label="Files" on_value_change=on_file>
                <Part part=tree_view::Part::Root>
                    {nodes
                        .into_iter()
                        .map(|path| {
                            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                            view! { <Part part=tree_view::Part::Node item=path>{name}</Part> }
                        })
                        .collect_view()}
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn CommandSection() -> impl IntoView {
    let (ran, on_ran) = follow();
    view! {
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
    }
}

#[component]
fn NumberSection() -> impl IntoView {
    let (quantity, on_quantity) = follow();
    view! {
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
    }
}

#[component]
fn SortableSection() -> impl IntoView {
    let (groceries, on_groceries) = follow();
    let shopping = RwSignal::new(vec!["milk", "eggs", "bread", "jam"]);
    Effect::new(move |_| {
        let order = groceries.get();
        if !order.is_empty() {
            shopping.update(|list| list.sort_by_key(|key| order.iter().position(|k| k == key)));
        }
    });
    view! {
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
    }
}

#[component]
fn CalendarSection() -> impl IntoView {
    let (day, on_day) = follow();
    let start = Date::parse("2026-11-17").unwrap_or(Date::FIRST);
    view! {
        <Section title="Calendar">
            <p id="day-value">"Day: " {move || day.get().join("")}</p>
            <Root machine=Calendar::new(armature_core::calendar::Options::default(), start) label="Date" on_value_change=on_day>
                <CalendarParts />
            </Root>
        </Section>
    }
}

#[component]
fn ToolbarSection() -> impl IntoView {
    let (pressed, on_pressed) = follow();
    view! {
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
    }
}

#[component]
fn OneTimeCodeSection() -> impl IntoView {
    let (code, on_code) = follow();
    view! {
        <Section title="One-time code">
            <p id="code-value">"Code: " {move || code.get().join("")}</p>
            <Root machine=OtpInput::new(4) label="Code" on_value_change=on_code>
                <div class="otp">
                    <Part part=otp_input::Part::Input />
                    <OtpCells />
                </div>
            </Root>
        </Section>
    }
}

#[component]
fn TokensSection() -> impl IntoView {
    let (tags, on_tags) = follow();
    view! {
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
    }
}

#[component]
fn ResizableSection() -> impl IntoView {
    let (split, on_split) = follow();
    view! {
        <Section title="Resizable">
            <p id="split-value">"Split: " {move || split.get().join("")}</p>
            <Root machine=Resizable::default() label="Panels" on_value_change=on_split>
                <Part part=resizable::Part::Root>
                    <Panels />
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn PopoverSection() -> impl IntoView {
    view! {
        <Section title="Popover">
            <div class="fade">
                <Root machine=Popover::default() label="Share">
                    <Part part=popover::Part::Trigger>"Share"</Part>
                    <Part part=popover::Part::Content>
                        <Part part=popover::Part::Arrow />
                        <Part part=popover::Part::Title>"Share this page"</Part>
                        <Part part=popover::Part::Description>"Anyone with the link can view."</Part>
                        <Part part=popover::Part::Close label="Close">"×"</Part>
                    </Part>
                </Root>
            </div>
        </Section>
    }
}

#[component]
fn TooltipSection() -> impl IntoView {
    view! {
        <Section title="Tooltip">
            <Root machine=Tooltip::default() label="Help">
                <Part part=tooltip::Part::Trigger>"Hover or focus me"</Part>
                <Part part=tooltip::Part::Content>"Saves your work"</Part>
            </Root>
            // Tooltips that sit together share their warmth: once one has
            // shown, its neighbour shows without waiting again.
            {
                let warmth = tooltip::Warmth::default();
                let tip = move |label: &'static str, text: &'static str| {
                    view! {
                        <Root machine=Tooltip::default().sharing(warmth.clone()) label=label>
                            <Part part=tooltip::Part::Trigger>{label}</Part>
                            <Part part=tooltip::Part::Content>{text}</Part>
                        </Root>
                    }
                };
                view! { <div id="warm-tips">{tip("Bold", "Make it bold")}{tip("Italic", "Make it italic")}</div> }
            }
        </Section>
    }
}

#[component]
fn ColourSection() -> impl IntoView {
    let (color, on_color) = follow();
    view! {
        <Section title="Colour">
            <p id="color-value">"Colour: " {move || color.get().join("")}</p>
            <Root machine=ColorPicker::default() label="Colour" on_value_change=on_color>
                <ColorParts />
            </Root>
        </Section>
    }
}

#[component]
fn SwipeRowSection() -> impl IntoView {
    let (swiped, on_swiped) = follow();
    view! {
        <Section title="Swipe row">
            <p id="swipe-value">"Ran: " {move || swiped.get().join("")}</p>
            <Root machine=SwipeRow::default() label="Message" on_value_change=on_swiped>
                <SwipeParts />
            </Root>
        </Section>
    }
}

#[component]
fn VirtualListSection() -> impl IntoView {
    let (row, on_row) = follow();
    view! {
        <Section title="Virtual list">
            <p id="row-value">"Row: " {move || row.get().join("")}</p>
            <Root machine=VirtualList::new(10000, 24.0) label="Rows" on_value_change=on_row>
                <VirtualRows />
            </Root>
        </Section>
    }
}

#[component]
fn TimelineSection() -> impl IntoView {
    let (day_plan, on_day_plan) = follow();
    view! {
        <Section title="Timeline">
            <p id="plan-value">"Plan: " {move || day_plan.get().join(" ")}</p>
            <Root machine=Timeline::default() label="Day" on_value_change=on_day_plan>
                <TimelineParts />
            </Root>
        </Section>
    }
}

#[component]
fn DataGridSection() -> impl IntoView {
    let (cells, on_cells) = follow();
    let sheet = ["a1", "b1", "c1", "a2", "b2", "c2", "a3", "b3", "c3"]
        .chunks(3)
        .map(|row| row.iter().map(std::string::ToString::to_string).collect())
        .collect::<Vec<Vec<String>>>();
    view! {
        <Section title="Data grid">
            <p id="cells-value">"Cells: " {move || cells.get().join(" ")}</p>
            <Root machine=DataGrid::new(sheet, Dir::Ltr) label="Sheet" on_value_change=on_cells>
                <GridCells />
            </Root>
        </Section>
    }
}

#[component]
fn CanvasSection() -> impl IntoView {
    let (layout, on_layout) = follow();
    let nodes = [("a", 10.0, 10.0), ("b", 150.0, 10.0), ("c", 10.0, 100.0)]
        .map(|(key, x, y)| Node {
            key: key.into(),
            rect: Rect::new(x, y, 90.0, 50.0),
        })
        .to_vec();
    view! {
        <Section title="Canvas">
            <p id="layout-value">{move || layout.get().join(" ")}</p>
            <Root machine=Canvas::new(nodes) label="Board" on_value_change=on_layout>
                <CanvasParts />
            </Root>
        </Section>
    }
}

#[component]
fn SheetSection() -> impl IntoView {
    let (rest, on_rest) = follow();
    let visible = use_visible_viewport();
    view! {
        <Section title="Sheet">
            <p id="sheet-value">"Rests at: " {move || rest.get().join("")}</p>
            <p id="visible-value">
                {move || format!("Visible: {}x{}", visible.get().width.round(), visible.get().height.round())}
            </p>
            <Root machine=Sheet::new(vec![120.0, 300.0]) label="Details" on_value_change=on_rest>
                <SheetParts />
            </Root>
        </Section>
    }
}

#[component]
fn DialogSection() -> impl IntoView {
    view! {
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
            // One that must be answered with its own button: Escape and a press
            // outside are refused.
            <Root machine=Dialog::new(dialog::Options { closes: dialog::Closes::Never, ..Default::default() }) label="Terms">
                <Part part=dialog::Part::Trigger>"Open terms"</Part>
                <Part part=dialog::Part::Backdrop />
                <Part part=dialog::Part::Content>
                    <p>"Accept the terms to continue."</p>
                    <Part part=dialog::Part::Close>"Accept"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn AgentRulesSection() -> impl IntoView {
    view! {
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
    }
}

#[component]
fn EditableSection() -> impl IntoView {
    let (title, on_title) = follow();
    view! {
        <Section title="Editable">
            <p id="title-value">"Title: " {move || title.get().join("")}</p>
            <Root machine=Editable::new("Untitled") label="Title" on_value_change=on_title>
                <Part part=editable::Part::Preview>{move || title.get().first().cloned().unwrap_or_else(|| "Untitled".into())}</Part>
                <Part part=editable::Part::Input />
            </Root>
        </Section>
    }
}

#[component]
fn PromptSection() -> impl IntoView {
    let sent = RwSignal::new(Vec::<String>::new());
    use_shortcut("mod+k", || {
        let field = document()
            .query_selector("[aria-label=\"Message\"]")
            .ok()
            .flatten();
        if let Some(field) = field.and_then(|field| {
            leptos::wasm_bindgen::JsCast::dyn_into::<leptos::web_sys::HtmlElement>(field).ok()
        }) {
            let _ = field.focus();
        }
    });
    view! {
        <Section title="Prompt">
            <p id="prompt-value">"Sent: " {move || sent.get().join(" | ")}</p>
            <Root
                machine=Prompt::default()
                label="Message"
                on_value_change=Callback::new(move |change: ValueChange| {
                    sent.update(|sent| sent.extend(change.value));
                    true
                })
            >
                <Part part=prompt::Part::Input />
            </Root>
            <p class="hint">"Ctrl+K focuses this field."</p>
        </Section>
    }
}

#[component]
fn HoldToConfirmSection() -> impl IntoView {
    let deleted = RwSignal::new(0_u32);
    view! {
        <Section title="Hold to confirm">
            <p id="hold-value">"Deleted: " {move || deleted.get()}</p>
            <Root
                machine=HoldButton::new(300)
                label="Delete"
                on_value_change=Callback::new(move |_: ValueChange| {
                    deleted.update(|count| *count = count.saturating_add(1));
                    true
                })
            >
                <Part part=hold_button::Part::Root>"Hold to delete"</Part>
            </Root>
        </Section>
    }
}

#[component]
fn TransferListSection() -> impl IntoView {
    let (chosen, on_chosen) = follow();
    view! {
        <Section title="Transfer list">
            <p id="transfer-value">"Chosen: " {move || chosen.get().join(", ")}</p>
            <Root machine=TransferList::new(&["read", "write", "share", "delete"]) label="Permissions" on_value_change=on_chosen>
                <TransferParts />
            </Root>
        </Section>
    }
}

#[component]
fn OutlineSection() -> impl IntoView {
    let (moved, on_moved) = follow();
    view! {
        <Section title="Outline">
            <p id="outline-value">"Moved: " {move || moved.get().join("")}</p>
            <Root machine=TreeView::new(tree_from_paths(&["src/main.rs", "src/ui/button.rs", "docs/intro.md", "README.md"]), Dir::Ltr) label="Outline" on_value_change=on_moved>
                <OutlineRows />
            </Root>
        </Section>
    }
}

#[component]
fn FindableSection() -> impl IntoView {
    view! {
        <Section title="Findable">
            <Root machine=Accordion::new(accordion::Options { findable: true, ..Default::default() }) label="Support">
                <Part part=accordion::Part::Trigger item="billing">"Billing"</Part>
                <Part part=accordion::Part::Panel item="billing">"Invoices are sent on the first."</Part>
                <Part part=accordion::Part::Trigger item="privacy">"Privacy"</Part>
                <Part part=accordion::Part::Panel item="privacy">"We keep nothing you did not send."</Part>
            </Root>
        </Section>
    }
}

#[component]
fn PickerSection() -> impl IntoView {
    let (hour, on_hour) = follow();
    view! {
        <Section title="Picker">
            <p id="hour-value">"Hour: " {move || hour.get().join("")}</p>
            <Root machine=Picker::new(28.0) label="Hour" on_value_change=on_hour>
                <Part part=picker::Part::Viewport>
                    <div class="picker-pad">
                        {(1..=12)
                            .map(|n| view! { <Part part=picker::Part::Option item=n.to_string()>{n}</Part> })
                            .collect_view()}
                    </div>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn ThemeSection() -> impl IntoView {
    view! {
        <Section title="Theme">
            <ThemeChoice />
        </Section>
    }
}

#[component]
fn App() -> impl IntoView {
    view! {
        <main>
            <SelectSection />
            <TabsAndListboxSection />
            <SliderSection />
            <SwitchSection />
            <AccordionSection />
            <RatingSection />
            <TreeSection />
            <CommandSection />
            <NumberSection />
            <SortableSection />
            <CalendarSection />
            <ToolbarSection />
            <OneTimeCodeSection />
            <TokensSection />
            <ResizableSection />
            <PopoverSection />
            <TooltipSection />
            <ColourSection />
            <SwipeRowSection />
            <VirtualListSection />
            <TimelineSection />
            <DataGridSection />
            <CanvasSection />
            <SheetSection />
            <DialogSection />
            <AgentRulesSection />
            <EditableSection />
            <PromptSection />
            <HoldToConfirmSection />
            <TransferListSection />
            <OutlineSection />
            <FindableSection />
            <PickerSection />
            <ThemeSection />
            <more::More />
            <audit::Audit />
        </main>
    }
}

fn main() {
    agent::install();
    // Whether the browser offers its tool API or not, the page works the same.
    let _ = agent::register_tools();
    mount_to_body(App);
}
