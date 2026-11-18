//! The components added to match UI SDK's set, each in its simplest form.

use armature_core::avatar::{self, Avatar};
use armature_core::button::{self, Button};
use armature_core::carousel::{self, Carousel};
use armature_core::checkbox::{self, Checkbox, CheckboxGroup};
use armature_core::collapsible::{self, Collapsible};
use armature_core::combobox::{self, Combobox};
use armature_core::date::Date;
use armature_core::date_input::{self, DateInput};
use armature_core::date_picker::{self, DatePicker};
use armature_core::file_input::{self, FileInput};
use armature_core::form::Form;
use armature_core::gauge::{Meter, MeterPart, Progress, ProgressPart, Separator, SeparatorPart};
use armature_core::geometry::Size;
use armature_core::image_cropper::{self, HANDLES, ImageCropper};
use armature_core::keys::Dir;
use armature_core::menu::{self, Entry, Kind as EntryKind, Menu};
use armature_core::menubar::{self, Menubar};
use armature_core::navigation_menu::{self, NavigationMenu};
use armature_core::pagination::{self, PageItem, Pagination};
use armature_core::scroll_area::{self, ScrollArea};
use armature_core::selection::Count;
use armature_core::text_field::{self, Kind, TextField};
use armature_core::toast::{self, Toast, Toaster};
use armature_core::toggle_group::{self, ToggleGroup};
use armature_core::tooltip::{self, Tooltip};
use armature_core::trigger_menu::{self, TriggerMenu};
use armature_core::waterfall::{self, Span, Waterfall};
use armature_leptos::{Part, Root, ValueChange, use_primitive};
use leptos::prelude::*;

use crate::{Section, follow};

fn entries() -> Vec<Entry> {
    vec![
        Entry::action("new", "New file"),
        Entry::action("wrap", "Word wrap").of(EntryKind::Checkbox),
        Entry::action("share", "Share").of(EntryKind::Submenu),
        Entry::action("email", "Email").inside("share"),
        Entry::action("link", "Copy link").inside("share"),
    ]
}

#[component]
fn MenuItems() -> impl IntoView {
    let menu = use_primitive::<Menu>();
    let shown = move || {
        menu.with(|m| {
            m.shown()
                .into_iter()
                .map(|entry| (entry.key.clone(), entry.label.clone()))
                .collect::<Vec<_>>()
        })
    };
    view! {
        <Part part=menu::Part::Content>
            <For each=shown key=|(key, _)| key.clone() let((key, label))>
                <Part part=menu::Part::Item item=key>{label}</Part>
            </For>
        </Part>
    }
}

#[component]
fn PromptContent() -> impl IntoView {
    use armature_core::token_editor::{Part as EditorPart, TokenEditor};
    use armature_core::token_text::Segment;
    let editor = use_primitive::<TokenEditor>();
    // Each piece with a key that changes when its content does.
    let pieces = move || {
        editor.with(|editor| {
            let (before, after) = editor.split_at_caret();
            let caret = std::iter::once((String::from("caret"), None, String::new()));
            let piece = |side: &'static str| {
                move |(at, segment): (usize, Segment)| match segment {
                    Segment::Text(text) => (format!("{side}{at}:{text}"), None, text),
                    Segment::Token(token) => {
                        (format!("token:{}", token.id), Some(token.id), token.label)
                    }
                }
            };
            before
                .into_iter()
                .enumerate()
                .map(piece("a"))
                .chain(caret)
                .chain(after.into_iter().enumerate().map(piece("b")))
                .collect::<Vec<_>>()
        })
    };
    view! {
        <Part part=EditorPart::Root>
            <span id="prompt-content" style="display:inline-block;min-width:16rem;min-height:1.4em;white-space:pre-wrap;border:1px solid currentColor;padding:2px 4px">
                <For each=pieces key=|(key, _, _)| key.clone() let((key, token, text))>
                    {match (key.as_str(), token) {
                        ("caret", _) => view! { <span data-caret="" style="border-left:1px solid currentColor;margin-right:-1px"></span> }.into_any(),
                        (_, Some(id)) => view! { <Part part=EditorPart::Token item=id><b>{text}</b></Part> }.into_any(),
                        _ => view! { <span>{text}</span> }.into_any(),
                    }}
                </For>
            </span>
        </Part>
    }
}

#[component]
fn OutlineRows() -> impl IntoView {
    use armature_core::outline::{Outline, Part as OutlinePart};
    let outline = use_primitive::<Outline>();
    let rows = move || {
        outline.with(|o| {
            o.rows()
                .iter()
                .map(|row| (row.key.clone(), row.depth))
                .collect::<Vec<_>>()
        })
    };
    view! {
        <Part part=OutlinePart::Root>
            <For each=rows key=|row| row.clone() let((key, depth))>
                <Part part=OutlinePart::Row item=key.clone()>
                    <span style=format!("display:block;padding-left:{}px", depth.saturating_mul(24))>{key}</span>
                </Part>
            </For>
        </Part>
    }
}

#[component]
fn BarMenus() -> impl IntoView {
    let bar = use_primitive::<Menubar>();
    let menu = move |key: &'static str, title: &'static str| {
        let shown = move || {
            bar.with(|b| {
                b.shown(key)
                    .into_iter()
                    .map(|entry| (entry.key.clone(), entry.label.clone()))
                    .collect::<Vec<_>>()
            })
        };
        view! {
            <Part part=menubar::Part::Trigger item=key>{title}</Part>
            <Part part=menubar::Part::Content item=key>
                <For each=shown key=|(entry, _)| entry.clone() let((entry, label))>
                    <Part part=menubar::Part::Item item=format!("{key}/{entry}")>{label}</Part>
                </For>
            </Part>
        }
    };
    view! { <Part part=menubar::Part::Root><div class="bar">{menu("file", "File")} {menu("view", "View")}</div></Part> }
}

#[component]
fn Pages() -> impl IntoView {
    let pages = use_primitive::<Pagination>();
    view! {
        <Part part=pagination::Part::Root>
            <div class="pages">
                <Part part=pagination::Part::Previous label="Previous page">"‹"</Part>
                <For each=move || pages.with(armature_core::pagination::Pagination::range) key=|item| format!("{item:?}") let:item>
                    {match item {
                        PageItem::Page(page) => view! { <Part part=pagination::Part::Page item=page.to_string()>{page}</Part> }.into_any(),
                        _ => view! { <span>"…"</span> }.into_any(),
                    }}
                </For>
                <Part part=pagination::Part::Next label="Next page">"›"</Part>
            </div>
        </Part>
    }
}

#[component]
fn Toasts() -> impl IntoView {
    let toaster = use_primitive::<Toaster>();
    view! {
        <Part part=toast::Part::Region>
            <For each=move || toaster.with(|t| t.visible().to_vec()) key=|toast| toast.id.clone() let:toast>
                <Part part=toast::Part::Toast item=toast.id.clone()>
                    <Part part=toast::Part::Title item=toast.id.clone()>{toast.title.clone()}</Part>
                    <Part part=toast::Part::Action item=toast.id.clone()>{toast.action.clone()}</Part>
                    <Part part=toast::Part::Close item=toast.id label="Dismiss">"×"</Part>
                </Part>
            </For>
        </Part>
    }
}

#[component]
fn PickerParts() -> impl IntoView {
    let picker = use_primitive::<DatePicker>();
    let weeks = move || picker.with(|p| p.calendar().weeks());
    view! {
        <div class="segments">
            <Part part=date_picker::Part::Segment item="year" label="Year" />
            "-"
            <Part part=date_picker::Part::Segment item="month" label="Month" />
            "-"
            <Part part=date_picker::Part::Segment item="day" label="Day" />
            <Part part=date_picker::Part::Trigger>"▾"</Part>
        </div>
        <Part part=date_picker::Part::Content>
            <Part part=date_picker::Part::Grid>
                <For each=weeks key=|week| week[0].format() let:week>
                    <div class="week">
                        {week.map(|day| view! { <Part part=date_picker::Part::Day item=day.format()>{day.day}</Part> }).collect_view()}
                    </div>
                </For>
            </Part>
        </Part>
    }
}

#[component]
fn Slides() -> impl IntoView {
    let carousel = use_primitive::<Carousel>();
    view! {
        <Part part=carousel::Part::Root>
            <Part part=carousel::Part::Viewport>
                <div class="strip" style:transform=move || format!("translateX(-{}%)", carousel.with(armature_core::carousel::Carousel::position) * 100.0)>
                    {["one", "two", "three"].map(|key| view! { <Part part=carousel::Part::Slide item=key>{key}</Part> }).collect_view()}
                </div>
            </Part>
            <Part part=carousel::Part::Previous label="Previous slide">"‹"</Part>
            <Part part=carousel::Part::Next label="Next slide">"›"</Part>
        </Part>
    }
}

#[component]
fn ShelfSlides() -> impl IntoView {
    let carousel = use_primitive::<Carousel>();
    let moved = move || {
        format!(
            "translateX(-{}px)",
            carousel.with(|c| c.strip_offset().unwrap_or(0.0))
        )
    };
    view! {
        <Part part=carousel::Part::Root>
            <Part part=carousel::Part::Viewport>
                <div>
                    <div id="shelf-strip" style="display:flex;gap:10px" style:transform=moved>
                        {["a", "b", "c", "d", "e", "f", "g"]
                            .map(|key| view! { <Part part=carousel::Part::Slide item=key><span style="display:block;width:100px;flex:0 0 100px">{key}</span></Part> })
                            .collect_view()}
                    </div>
                </div>
            </Part>
            <Part part=carousel::Part::Previous label="Previous shelf">"‹"</Part>
            <Part part=carousel::Part::Next label="Next shelf">"›"</Part>
        </Part>
    }
}

#[component]
fn ScrollParts() -> impl IntoView {
    let area = use_primitive::<ScrollArea>();
    // Where the thumb starts along its bar, and how long it is.
    let thumb = move || area.with(|a| a.thumb(120.0));
    let thumb_start = move || {
        let (start, _) = thumb();
        format!("{start}px")
    };
    let thumb_length = move || {
        let (_, length) = thumb();
        format!("{length}px")
    };
    view! {
        <Part part=scroll_area::Part::Root>
            <div class="scroll-area">
                <Part part=scroll_area::Part::Viewport>
                    {(1..=30).map(|n| view! { <p>"Line " {n}</p> }).collect_view()}
                </Part>
                <Part part=scroll_area::Part::Scrollbar>
                    <div class="thumb-place" style:top=thumb_start style:height=thumb_length>
                        <Part part=scroll_area::Part::Thumb />
                    </div>
                </Part>
            </div>
        </Part>
    }
}

#[component]
fn WideParts() -> impl IntoView {
    let area = use_primitive::<ScrollArea>();
    // Where the thumb starts along its bar, and how long it is.
    let thumb = move || area.with(|a| a.thumb(192.0));
    let thumb_start = move || {
        let (start, _) = thumb();
        format!("{start}px")
    };
    let thumb_length = move || {
        let (_, length) = thumb();
        format!("{length}px")
    };
    view! {
        <Part part=scroll_area::Part::Root>
            <div class="scroll-wide">
                <Part part=scroll_area::Part::Viewport>
                    <div class="wide-strip">{(1..=20).map(|n| view! { <span>"Day " {n}</span> }).collect_view()}</div>
                </Part>
                <Part part=scroll_area::Part::Scrollbar>
                    <div class="thumb-across" style:left=thumb_start style:width=thumb_length>
                        <Part part=scroll_area::Part::Thumb />
                    </div>
                </Part>
            </div>
        </Part>
    }
}

/// Where to draw a cropper's selection over its picture: its left, top, width and
/// height as shares of what is shown.
pub fn crop_box(
    cropper: ReadSignal<ImageCropper>,
) -> (
    impl Fn() -> String + Copy,
    impl Fn() -> String + Copy,
    impl Fn() -> String + Copy,
    impl Fn() -> String + Copy,
) {
    let share = move |pick: fn([f64; 4]) -> f64| {
        move || format!("{}%", cropper.with(|c| pick(c.fractions())) * 100.0)
    };
    (
        share(|[left, _, _, _]| left),
        share(|[_, top, _, _]| top),
        share(|[_, _, width, _]| width),
        share(|[_, _, _, height]| height),
    )
}

#[component]
fn CropParts() -> impl IntoView {
    let cropper = use_primitive::<ImageCropper>();
    let (left, top, width, height) = crop_box(cropper);
    view! {
        <Part part=image_cropper::Part::Area>
            <div class="crop-box" style:left=left style:top=top style:width=width style:height=height>
                <Part part=image_cropper::Part::Selection label="Crop">
                    {HANDLES.map(|name| view! { <Part part=image_cropper::Part::Handle item=name label=name /> }).collect_view()}
                </Part>
            </div>
        </Part>
    }
}

#[component]
fn TraceRows() -> impl IntoView {
    let trace = use_primitive::<Waterfall>();
    // Where a row's bar starts and how long it is, as percentages of the lane.
    let bar_start = move |key: String| {
        move || {
            let (start, _) = trace.with(|t| t.bar(&key)).unwrap_or_default();
            format!("{}%", start * 100.0)
        }
    };
    let bar_length = move |key: String| {
        move || {
            let (_, length) = trace.with(|t| t.bar(&key)).unwrap_or_default();
            format!("{}%", length * 100.0)
        }
    };
    view! {
        <Part part=waterfall::Part::Root>
            <For each=move || trace.with(armature_core::waterfall::Waterfall::rows) key=|row| row.key.clone() let:row>
                {
                    // The markup takes the key in more places than it can be cloned for in
                    // order, so the shown name is a copy of its own.
                    let (key, name) = (row.key.clone(), row.key.clone());
                    view! {
                        <div class="trace-row">
                            <div class="trace-name" style:padding-left=format!("{}px", row.level.saturating_sub(1).saturating_mul(12))>
                                <Part part=waterfall::Part::Row item=key.clone()>{name}</Part>
                            </div>
                            <div class="trace-lane">
                                <div class="trace-bar" style:left=bar_start(key.clone()) style:width=bar_length(key)>
                                    <Part part=waterfall::Part::Bar item=key.clone() />
                                </div>
                            </div>
                        </div>
                    }
                }
            </For>
        </Part>
    }
}

#[component]
fn Files() -> impl IntoView {
    let input = use_primitive::<FileInput>();
    view! {
        <Part part=file_input::Part::Dropzone>"Drop files here, or press to browse"</Part>
        <Part part=file_input::Part::List>
            <For each=move || input.with(|i| i.files().to_vec()) key=|file| file.name.clone() let:file>
                <Part part=file_input::Part::File item=file.name.clone()>
                    {file.name.clone()}
                    <Part part=file_input::Part::Remove item=file.name.clone() label="Remove">"×"</Part>
                </Part>
            </For>
        </Part>
    }
}

#[component]
fn ButtonSection() -> impl IntoView {
    let saves = RwSignal::new(0_u32);
    let on_save = Callback::new(move |_: ValueChange| {
        saves.update(|count| *count = count.saturating_add(1));
        true
    });
    view! {
        <Section title="Button">
            <p id="button-value">"Saves: " {move || saves.get()}</p>
            <Root machine=Button::default() label="Save" on_value_change=on_save>
                <Part part=button::Part::Root>"Save"</Part>
            </Root>
        </Section>
    }
}

#[component]
fn MeterAndProgressSection() -> impl IntoView {
    view! {
        <Section title="Meter and progress">
            <Root machine=Meter::new(40.0, 0.0, 200.0) label="Storage"><Part part=MeterPart::Root>"20% used"</Part></Root>
            <Root machine=Progress::new(Some(60.0), 0.0, 100.0) label="Upload"><Part part=ProgressPart::Root>"60%"</Part></Root>
            <Root machine=Separator::default() label="Rule"><Part part=SeparatorPart::Root /></Root>
        </Section>
    }
}

#[component]
fn CollapsibleSection() -> impl IntoView {
    view! {
        <Section title="Collapsible">
            <Root machine=Collapsible::default() label="Details">
                <Part part=collapsible::Part::Trigger>"Details"</Part>
                <Part part=collapsible::Part::Panel>"More to read."</Part>
            </Root>
        </Section>
    }
}

#[component]
fn CheckboxSection() -> impl IntoView {
    let (agreed, on_agreed) = follow();
    let (alerts, on_alerts) = follow();
    view! {
        <Section title="Checkbox">
            <p id="agreed-value">"Agreed: " {move || agreed.get().join("")}</p>
            <Root machine=Checkbox::default() label="Agree" on_value_change=on_agreed>
                <Part part=checkbox::Part::Root><Part part=checkbox::Part::Indicator>"✓"</Part></Part>
            </Root>
            <p id="alerts-value">"Alerts: " {move || alerts.get().join(", ")}</p>
            <Root machine=CheckboxGroup::default() label="Alerts" on_value_change=on_alerts>
                <Part part=checkbox::GroupPart::Root>
                    <Part part=checkbox::GroupPart::All label="All alerts">"All"</Part>
                    <Part part=checkbox::GroupPart::Option item="email">"Email"</Part>
                    <Part part=checkbox::GroupPart::Option item="sms">"SMS"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn ToggleGroupSection() -> impl IntoView {
    let (align, on_align) = follow();
    view! {
        <Section title="Toggle group">
            <p id="align-value">"Align: " {move || align.get().join("")}</p>
            <Root machine=ToggleGroup::default() label="Align" on_value_change=on_align>
                <Part part=toggle_group::Part::Root>
                    <Part part=toggle_group::Part::Toggle item="left">"Left"</Part>
                    <Part part=toggle_group::Part::Toggle item="center">"Centre"</Part>
                    <Part part=toggle_group::Part::Toggle item="right">"Right"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn PaginationSection() -> impl IntoView {
    let (page, on_page) = follow();
    view! {
        <Section title="Pagination">
            <p id="page-value">"Page: " {move || page.get().join("")}</p>
            <Root machine=Pagination::new(1, 10) label="Results" on_value_change=on_page><Pages /></Root>
        </Section>
    }
}

#[component]
fn TextFieldSection() -> impl IntoView {
    let (email, on_email) = follow();
    view! {
        <Section title="Text field">
            <p id="email-value">"Email: " {move || email.get().join("")}</p>
            <Root
                machine=TextField::new(Kind::Text).required().validate(|text| (!text.contains('@')).then(|| "Enter an email address.".to_string()))
                label="Email"
                on_value_change=on_email
            >
                <Part part=text_field::Part::Label>"Email"</Part>
                <Part part=text_field::Part::Input />
                <Part part=text_field::Part::Clear label="Clear">"×"</Part>
                <Part part=text_field::Part::Error>"Enter an email address."</Part>
            </Root>
            <Root machine=TextField::new(Kind::Password) label="Password" agent_secret=true>
                <Part part=text_field::Part::Input />
                <Part part=text_field::Part::Reveal label="Show password">"👁"</Part>
            </Root>
        </Section>
    }
}

#[component]
fn AvatarSection() -> impl IntoView {
    view! {
        <Section title="Avatar">
            <Root machine=Avatar::loading() label="Ada Lovelace">
                <Part part=avatar::Part::Root>
                    <Part part=avatar::Part::Image><img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==" width="32" height="32" alt="" /></Part>
                    <Part part=avatar::Part::Fallback>"AL"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn MenuSection() -> impl IntoView {
    let (ran, on_ran) = follow();
    view! {
        <Section title="Menu">
            <p id="menu-value">"Menu: " {move || ran.get().join(", ")}</p>
            <Root machine=Menu::new(entries()) label="File actions" on_value_change=on_ran>
                <Part part=menu::Part::Trigger>"Actions"</Part>
                <Part part=menu::Part::Area><span class="context-area">"Or right-click here"</span></Part>
                <MenuItems />
            </Root>
        </Section>
    }
}

#[component]
fn MenubarSection() -> impl IntoView {
    let (bar, on_bar) = follow();
    view! {
        <Section title="Menubar">
            <p id="bar-value">"Bar: " {move || bar.get().join(", ")}</p>
            <Root
                machine=Menubar::new(vec![("file", "File", entries()), ("view", "View", vec![Entry::action("zoom", "Zoom in")])], Dir::Ltr)
                label="Application"
                on_value_change=on_bar
            >
                <BarMenus />
            </Root>
        </Section>
    }
}

#[component]
fn HoverCardSection() -> impl IntoView {
    view! {
        <Section title="Hover card">
            <Root machine=Tooltip::hover_card() label="Profile">
                <Part part=tooltip::Part::Trigger>"@ada"</Part>
                <Part part=tooltip::Part::Content>"Ada Lovelace, mathematician."</Part>
            </Root>
        </Section>
    }
}

#[component]
fn ComboboxSection() -> impl IntoView {
    let (fruit, on_fruit) = follow();
    let loading = RwSignal::new(false);
    let fruit_settings = Signal::derive(move || {
        vec![(
            "status".to_string(),
            if loading.get() { "loading" } else { "idle" }.to_string(),
        )]
    });
    let toggle_loading = Callback::new(move |_: ValueChange| {
        loading.update(|on| *on = !*on);
        true
    });
    view! {
        <Section title="Combobox">
            <p id="combo-value">"Fruit: " {move || fruit.get().join(", ")}</p>
            <Root machine=Button::default() label="Load fruit" on_value_change=toggle_loading><Part part=button::Part::Root>"Load"</Part></Root>
            <Root
                machine=Combobox::new(combobox::Options { auto_highlight: true, ..Default::default() })
                label="Favourite fruit"
                on_value_change=on_fruit
                settings=fruit_settings
            >
                <Part part=combobox::Part::Input />
                <Part part=combobox::Part::List>
                    <Part part=combobox::Part::Option item="apple">"Apple"</Part>
                    <Part part=combobox::Part::Option item="apricot">"Apricot"</Part>
                    <Part part=combobox::Part::Option item="banana">"Banana"</Part>
                    <Part part=combobox::Part::Empty>"Nothing matches."</Part>
                    <Part part=combobox::Part::Status>"Loading…"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn NavigationMenuSection() -> impl IntoView {
    let (nav, on_nav) = follow();
    view! {
        <Section title="Navigation menu">
            <p id="nav-value">"Open: " {move || nav.get().join("")}</p>
            <Root machine=NavigationMenu::new(navigation_menu::Options::default(), &[("products", true), ("pricing", false)]).current("pricing") label="Main" on_value_change=on_nav>
                <Part part=navigation_menu::Part::Root>
                    <div class="bar">
                        <Part part=navigation_menu::Part::Trigger item="products">"Products"</Part>
                        <Part part=navigation_menu::Part::Link item="pricing">"Pricing"</Part>
                    </div>
                    <Part part=navigation_menu::Part::Panel item="products">"Analytics, storage and more."</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn ToastsSection() -> impl IntoView {
    let toasts = RwSignal::new(Vec::<String>::new());
    let count = RwSignal::new(0_u32);
    let on_toasts = Callback::new(move |change: ValueChange| {
        // An action is offered first. The list follows when the toast goes.
        if !change
            .value
            .first()
            .is_some_and(|entry| entry.starts_with("action:"))
        {
            toasts.set(change.value);
        }
        true
    });
    let notify = Callback::new(move |_: ValueChange| {
        count.update(|n| *n = n.saturating_add(1));
        let toast = Toast::new(&count.get_untracked().to_string(), "Saved")
            .action("Undo")
            .lasting(1500);
        toasts.update(|list| list.push(toast.encode()));
        true
    });
    view! {
        <Section title="Toasts">
            <p id="toast-value">"Showing: " {move || toasts.get().len()}</p>
            <Root machine=Button::default() label="Notify" on_value_change=notify><Part part=button::Part::Root>"Notify"</Part></Root>
            <Root machine=Toaster::default() label="Notifications" value=toasts on_value_change=on_toasts><Toasts /></Root>
        </Section>
    }
}

#[component]
fn DateInputSection() -> impl IntoView {
    let (when, on_when) = follow();
    view! {
        <Section title="Date input">
            <p id="when-value">"When: " {move || when.get().join("")}</p>
            <Root machine=DateInput::new(false) label="When" on_value_change=on_when>
                <Part part=date_input::Part::Root>
                    <div class="segments">
                        <Part part=date_input::Part::Segment item="year" label="Year" />
                        "-"
                        <Part part=date_input::Part::Segment item="month" label="Month" />
                        "-"
                        <Part part=date_input::Part::Segment item="day" label="Day" />
                    </div>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn DatePickerSection() -> impl IntoView {
    let (due, on_due) = follow();
    view! {
        <Section title="Date picker">
            <p id="due-value">"Due: " {move || due.get().join("")}</p>
            <Root machine=DatePicker::new(armature_core::calendar::Options::default(), Date::new(2026, 11, 17).unwrap_or(Date::FIRST)) label="Due date" on_value_change=on_due>
                <PickerParts />
            </Root>
        </Section>
    }
}

#[component]
fn MentionsSection() -> impl IntoView {
    let (note, on_note) = follow();
    view! {
        <Section title="Mentions">
            <p id="note-value">"Note: " {move || note.get().join("")}</p>
            <Root machine=TriggerMenu::new(&['@']) label="Note" on_value_change=on_note>
                <Part part=trigger_menu::Part::Input />
                <Part part=trigger_menu::Part::List>
                    <Part part=trigger_menu::Part::Option item="@ada">"ada"</Part>
                    <Part part=trigger_menu::Part::Option item="@alan">"alan"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
fn CarouselSection() -> impl IntoView {
    let (slide, on_slide) = follow();
    view! {
        <Section title="Carousel">
            <p id="slide-value">"Slide: " {move || slide.get().join("")}</p>
            <Root machine=Carousel::default() label="Gallery" on_value_change=on_slide><Slides /></Root>
        </Section>
    }
}

#[component]
fn ScrollAreaSection() -> impl IntoView {
    view! {
        <Section title="Scroll area">
            <Root machine=ScrollArea::default() label="Log"><ScrollParts /></Root>
            <Root machine=ScrollArea::horizontal() label="Timeline strip"><WideParts /></Root>
        </Section>
    }
}

#[component]
fn FormSection() -> impl IntoView {
    // A form keeps its fields' values together and says what stops a submit.
    let form = StoredValue::new(
        Form::default()
            .field("name", &[], true)
            .field("team", &[], false),
    );
    let outcome = RwSignal::new("Not sent".to_string());
    let name_problem = RwSignal::new(String::new());
    // What, if anything, stops the name being sent.
    let show_name_problem = move || {
        name_problem.set(
            form.with_value(|form| form.error("name"))
                .unwrap_or_default(),
        );
    };
    let field = move |name: &'static str| {
        Callback::new(move |change: ValueChange| {
            form.update_value(|form| form.set(name, change.value));
            show_name_problem();
            true
        })
    };
    let submit = Callback::new(move |_: ValueChange| {
        let result = form.try_update_value(armature_core::form::Form::submit);
        outcome.set(match result {
            Some(Ok(fields)) => format!(
                "Sent: {}",
                fields
                    .iter()
                    .map(|(name, value)| format!("{name}={}", value.join("")))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(Err(invalid)) => format!("Fix: {}", invalid.first),
            None => String::new(),
        });
        show_name_problem();
        true
    });
    let reset = Callback::new(move |_: ValueChange| {
        form.update_value(Form::reset);
        outcome.set("Not sent".to_string());
        name_problem.set(String::new());
        true
    });
    view! {
        <Section title="Form">
            <p id="form-value">{move || outcome.get()}</p>
            <Root machine=TextField::new(Kind::Text) label="Name" on_value_change=field("name")>
                <Part part=text_field::Part::Label>"Name"</Part>
                <Part part=text_field::Part::Input />
            </Root>
            <p id="form-name-error" class="problem">{move || name_problem.get()}</p>
            <Root machine=TextField::new(Kind::Text) label="Team" on_value_change=field("team")>
                <Part part=text_field::Part::Label>"Team"</Part>
                <Part part=text_field::Part::Input />
            </Root>
            <Root machine=Button::default() label="Create" on_value_change=submit><Part part=button::Part::Root>"Create"</Part></Root>
            <Root machine=Button::default() label="Reset" on_value_change=reset><Part part=button::Part::Root>"Reset"</Part></Root>
        </Section>
    }
}

#[component]
fn ImageCropperSection() -> impl IntoView {
    let (crop, on_crop) = follow();
    view! {
        <Section title="Image cropper">
            <p id="crop-value">"Crop: " {move || crop.get().join("")}</p>
            <Root machine=ImageCropper::new(Size { width: 800.0, height: 600.0 }, None) label="Crop" on_value_change=on_crop><CropParts /></Root>
        </Section>
    }
}

#[component]
fn FileInputSection() -> impl IntoView {
    let (files, on_files) = follow();
    view! {
        <Section title="File input">
            <p id="files-value">"Files: " {move || files.get().join(", ")}</p>
            <Root
                machine=FileInput::new(file_input::Options { count: Count::ANY, accept: vec!["image/*".into()], ..Default::default() })
                label="Attachments"
                on_value_change=on_files
            >
                <Files />
            </Root>
        </Section>
    }
}

#[component]
fn WaterfallSection() -> impl IntoView {
    let (span, on_span) = follow();
    let trace = vec![
        Span::new("page", 0.0, 400.0, None),
        Span::new("api", 100.0, 200.0, Some("page")),
        Span::new("db", 150.0, 100.0, Some("api")),
    ];
    view! {
        <Section title="Waterfall">
            <p id="span-value">"Span: " {move || span.get().join("")}</p>
            <Root machine=Waterfall::new(trace, Dir::Ltr) label="Trace" on_value_change=on_span><TraceRows /></Root>
        </Section>
    }
}

#[component]
fn UpstreamOptionsSection() -> impl IntoView {
    view! {
        <Section title="Upstream options">
            // A menu whose trigger is disabled never opens by hand.
            <Root machine=Menu::new(entries()).disabled() label="Locked actions">
                <Part part=menu::Part::Trigger>"Locked"</Part>
                <Part part=menu::Part::Content><span>"Never shown"</span></Part>
            </Root>
            // A menu that closes when a checkbox is chosen, with an entry that
            // destroys something.
            <Root
                machine=Menu::new(vec![
                    Entry::action("pin", "Pin").of(EntryKind::Checkbox),
                    Entry::action("erase", "Erase all").destructive(),
                ]).closing_on_select()
                label="Note actions"
            >
                <Part part=menu::Part::Trigger>"Note"</Part>
                <MenuItems />
            </Root>
            // Text with tokens in it, edited at a caret.
            <Root machine=armature_core::token_editor::TokenEditor::default() label="Prompt">
                <PromptContent />
            </Root>
            // A carousel whose pages come from its slides as measured: as many
            // as the viewport shows.
            <Root machine=Carousel::new(carousel::Options { fit_pages: true, ..Default::default() }) label="Shelf">
                <ShelfSlides />
            </Root>
            // An outline reordered and nested with the keyboard or by dragging.
            <Root
                machine=armature_core::outline::Outline::new(
                    armature_core::outline::Options::default(),
                    [("inbox", 0), ("projects", 0), ("site", 1), ("copy", 2), ("app", 1), ("archive", 0)]
                        .iter()
                        .map(|(key, depth)| armature_core::outline::OutlineRow { key: key.to_string(), depth: *depth })
                        .collect(),
                )
                label="Notes outline"
            >
                <OutlineRows />
            </Root>
            // A popover that keeps the page behind it out of reach.
            <Root machine=armature_core::popover::Popover::default().modal() label="Modal details">
                <Part part=armature_core::popover::Part::Trigger>"Details"</Part>
                <Part part=armature_core::popover::Part::Content>
                    <p>"Modal while open."</p>
                    <Part part=armature_core::popover::Part::Close>"Done"</Part>
                </Part>
            </Root>
        </Section>
    }
}

#[component]
pub fn More() -> impl IntoView {
    view! {
        <ButtonSection />
        <MeterAndProgressSection />
        <CollapsibleSection />
        <CheckboxSection />
        <ToggleGroupSection />
        <PaginationSection />
        <TextFieldSection />
        <AvatarSection />
        <MenuSection />
        <MenubarSection />
        <HoverCardSection />
        <ComboboxSection />
        <NavigationMenuSection />
        <ToastsSection />
        <DateInputSection />
        <DatePickerSection />
        <MentionsSection />
        <CarouselSection />
        <ScrollAreaSection />
        <FormSection />
        <ImageCropperSection />
        <FileInputSection />
        <WaterfallSection />
        <UpstreamOptionsSection />
    }
}
