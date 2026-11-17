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
use armature_core::gauge::{Meter, MeterPart, Progress, ProgressPart, Separator, SeparatorPart};
use armature_core::geometry::Size;
use armature_core::image_cropper::{self, HANDLES, ImageCropper};
use armature_core::keys::Dir;
use armature_core::menu::{self, Entry, Kind as EntryKind, Menu};
use armature_core::menubar::{self, Menubar};
use armature_core::navigation_menu::{self, NavigationMenu};
use armature_core::pagination::{self, PageItem, Pagination};
use armature_core::scroll_area::{self, ScrollArea};
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
            <For each=shown key=|entry| entry.0.clone() let:entry>
                <Part part=menu::Part::Item item=entry.0.clone()>{entry.1.clone()}</Part>
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
                <For each=shown key=|entry| entry.0.clone() let:entry>
                    <Part part=menubar::Part::Item item=format!("{key}/{}", entry.0)>{entry.1.clone()}</Part>
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
                <For each=move || pages.with(|p| p.range()) key=|item| format!("{item:?}") let:item>
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
                    <Part part=toast::Part::Close item=toast.id.clone() label="Dismiss">"×"</Part>
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
                <div class="strip" style:transform=move || format!("translateX(-{}%)", carousel.with(|c| c.position()) * 100.0)>
                    {["one", "two", "three"].map(|key| view! { <Part part=carousel::Part::Slide item=key>{key}</Part> }).collect_view()}
                </div>
            </Part>
            <Part part=carousel::Part::Previous label="Previous slide">"‹"</Part>
            <Part part=carousel::Part::Next label="Next slide">"›"</Part>
        </Part>
    }
}

#[component]
fn ScrollParts() -> impl IntoView {
    let area = use_primitive::<ScrollArea>();
    let thumb = move || area.with(|a| a.thumb(120.0));
    view! {
        <Part part=scroll_area::Part::Root>
            <div class="scroll-area">
                <Part part=scroll_area::Part::Viewport>
                    {(1..=30).map(|n| view! { <p>"Line " {n}</p> }).collect_view()}
                </Part>
                <Part part=scroll_area::Part::Scrollbar>
                    <div class="thumb-place" style:top=move || format!("{}px", thumb().0) style:height=move || format!("{}px", thumb().1)>
                        <Part part=scroll_area::Part::Thumb />
                    </div>
                </Part>
            </div>
        </Part>
    }
}

#[component]
fn CropParts() -> impl IntoView {
    let cropper = use_primitive::<ImageCropper>();
    let side = move |at: usize| move || format!("{}%", cropper.with(|c| c.fractions()[at]) * 100.0);
    view! {
        <Part part=image_cropper::Part::Area>
            <div class="crop-box" style:left=side(0) style:top=side(1) style:width=side(2) style:height=side(3)>
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
    let bar = move |key: String, at: usize| {
        move || {
            let (start, length) = trace.with(|t| t.bar(&key)).unwrap_or_default();
            format!("{}%", if at == 0 { start } else { length } * 100.0)
        }
    };
    view! {
        <Part part=waterfall::Part::Root>
            <For each=move || trace.with(|t| t.rows()) key=|row| row.key.clone() let:row>
                {
                    let (key, name) = (row.key.clone(), row.key.clone());
                    view! {
                        <div class="trace-row">
                            <div class="trace-name" style:padding-left=format!("{}px", (row.level - 1) * 12)>
                                <Part part=waterfall::Part::Row item=key.clone()>{name}</Part>
                            </div>
                            <div class="trace-lane">
                                <div class="trace-bar" style:left=bar(key.clone(), 0) style:width=bar(key.clone(), 1)>
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
pub fn More() -> impl IntoView {
    let saves = RwSignal::new(0);
    let on_save = Callback::new(move |_: ValueChange| {
        saves.update(|count| *count += 1);
        true
    });
    let (agreed, on_agreed) = follow();
    let (alerts, on_alerts) = follow();
    let (align, on_align) = follow();
    let (page, on_page) = follow();
    let (email, on_email) = follow();
    let (ran, on_ran) = follow();
    let (bar, on_bar) = follow();
    let (fruit, on_fruit) = follow();
    let (when, on_when) = follow();
    let (due, on_due) = follow();
    let (note, on_note) = follow();
    let (slide, on_slide) = follow();
    let (crop, on_crop) = follow();
    let (span, on_span) = follow();
    let (files, on_files) = follow();
    let (nav, on_nav) = follow();
    let toasts = RwSignal::new(Vec::<String>::new());
    let count = RwSignal::new(0);
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
        count.update(|n| *n += 1);
        let toast = Toast::new(&count.get_untracked().to_string(), "Saved")
            .action("Undo")
            .lasting(1500);
        toasts.update(|list| list.push(toast.encode()));
        true
    });
    let trace = vec![
        Span::new("page", 0.0, 400.0, None),
        Span::new("api", 100.0, 200.0, Some("page")),
        Span::new("db", 150.0, 100.0, Some("api")),
    ];

    view! {
        <Section title="Button">
            <p id="button-value">"Saves: " {move || saves.get()}</p>
            <Root machine=Button::default() label="Save" on_value_change=on_save>
                <Part part=button::Part::Root>"Save"</Part>
            </Root>
        </Section>

        <Section title="Meter and progress">
            <Root machine=Meter::new(40.0, 0.0, 200.0) label="Storage"><Part part=MeterPart::Root>"20% used"</Part></Root>
            <Root machine=Progress::new(Some(60.0), 0.0, 100.0) label="Upload"><Part part=ProgressPart::Root>"60%"</Part></Root>
            <Root machine=Separator::default() label="Rule"><Part part=SeparatorPart::Root /></Root>
        </Section>

        <Section title="Collapsible">
            <Root machine=Collapsible::default() label="Details">
                <Part part=collapsible::Part::Trigger>"Details"</Part>
                <Part part=collapsible::Part::Panel>"More to read."</Part>
            </Root>
        </Section>

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

        <Section title="Pagination">
            <p id="page-value">"Page: " {move || page.get().join("")}</p>
            <Root machine=Pagination::new(1, 10) label="Results" on_value_change=on_page><Pages /></Root>
        </Section>

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

        <Section title="Avatar">
            <Root machine=Avatar::loading() label="Ada Lovelace">
                <Part part=avatar::Part::Root>
                    <Part part=avatar::Part::Image><img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==" width="32" height="32" alt="" /></Part>
                    <Part part=avatar::Part::Fallback>"AL"</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Menu">
            <p id="menu-value">"Menu: " {move || ran.get().join(", ")}</p>
            <Root machine=Menu::new(entries()) label="File actions" on_value_change=on_ran>
                <Part part=menu::Part::Trigger>"Actions"</Part>
                <MenuItems />
            </Root>
        </Section>

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

        <Section title="Hover card">
            <Root machine=Tooltip::hover_card() label="Profile">
                <Part part=tooltip::Part::Trigger>"@ada"</Part>
                <Part part=tooltip::Part::Content>"Ada Lovelace, mathematician."</Part>
            </Root>
        </Section>

        <Section title="Combobox">
            <p id="combo-value">"Fruit: " {move || fruit.get().join(", ")}</p>
            <Root machine=Combobox::new(combobox::Options { auto_highlight: true, ..Default::default() }) label="Favourite fruit" on_value_change=on_fruit>
                <Part part=combobox::Part::Input />
                <Part part=combobox::Part::List>
                    <Part part=combobox::Part::Option item="apple">"Apple"</Part>
                    <Part part=combobox::Part::Option item="apricot">"Apricot"</Part>
                    <Part part=combobox::Part::Option item="banana">"Banana"</Part>
                    <Part part=combobox::Part::Empty>"Nothing matches."</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Navigation menu">
            <p id="nav-value">"Open: " {move || nav.get().join("")}</p>
            <Root machine=NavigationMenu::new(Default::default(), &[("products", true), ("pricing", false)]).current("pricing") label="Main" on_value_change=on_nav>
                <Part part=navigation_menu::Part::Root>
                    <div class="bar">
                        <Part part=navigation_menu::Part::Trigger item="products">"Products"</Part>
                        <Part part=navigation_menu::Part::Link item="pricing">"Pricing"</Part>
                    </div>
                    <Part part=navigation_menu::Part::Panel item="products">"Analytics, storage and more."</Part>
                </Part>
            </Root>
        </Section>

        <Section title="Toasts">
            <p id="toast-value">"Showing: " {move || toasts.get().len()}</p>
            <Root machine=Button::default() label="Notify" on_value_change=notify><Part part=button::Part::Root>"Notify"</Part></Root>
            <Root machine=Toaster::default() label="Notifications" value=toasts on_value_change=on_toasts><Toasts /></Root>
        </Section>

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

        <Section title="Date picker">
            <p id="due-value">"Due: " {move || due.get().join("")}</p>
            <Root machine=DatePicker::new(Default::default(), Date::new(2026, 11, 17).expect("a real date")) label="Due date" on_value_change=on_due>
                <PickerParts />
            </Root>
        </Section>

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

        <Section title="Carousel">
            <p id="slide-value">"Slide: " {move || slide.get().join("")}</p>
            <Root machine=Carousel::default() label="Gallery" on_value_change=on_slide><Slides /></Root>
        </Section>

        <Section title="Scroll area">
            <Root machine=ScrollArea::default() label="Log"><ScrollParts /></Root>
        </Section>

        <Section title="Image cropper">
            <p id="crop-value">"Crop: " {move || crop.get().join("")}</p>
            <Root machine=ImageCropper::new(Size { width: 800.0, height: 600.0 }, None) label="Crop" on_value_change=on_crop><CropParts /></Root>
        </Section>

        <Section title="File input">
            <p id="files-value">"Files: " {move || files.get().join(", ")}</p>
            <Root
                machine=FileInput::new(file_input::Options { multiple: true, accept: vec!["image/*".into()], ..Default::default() })
                label="Attachments"
                on_value_change=on_files
            >
                <Files />
            </Root>
        </Section>

        <Section title="Waterfall">
            <p id="span-value">"Span: " {move || span.get().join("")}</p>
            <Root machine=Waterfall::new(trace, Dir::Ltr) label="Trace" on_value_change=on_span><TraceRows /></Root>
        </Section>
    }
}
