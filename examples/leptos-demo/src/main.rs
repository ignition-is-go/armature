use armature_core::listbox::{self, Listbox};
use armature_core::select::{self, Select};
use armature_core::selection::Mode;
use armature_core::tabs::{self, Tabs};
use armature_leptos::{Part, Root, Source, ValueChange, agent};
use leptos::prelude::*;

#[component]
fn App() -> impl IntoView {
    let fruit = RwSignal::new(Vec::<String>::new());
    let access = RwSignal::new(Vec::<String>::new());

    let on_fruit = Callback::new(move |change: ValueChange| {
        fruit.set(change.value);
        true
    });
    let size = RwSignal::new(Vec::<String>::new());
    let on_size = Callback::new(move |change: ValueChange| {
        size.set(change.value);
        true
    });
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
        <main>
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

            <Root machine=Tabs::default() label="Settings">
                <Part part=tabs::Part::List>
                    <Part part=tabs::Part::Tab item="fruit">"Fruit"</Part>
                    <Part part=tabs::Part::Tab item="billing" disabled=true>
                        "Billing"
                    </Part>
                    <Part part=tabs::Part::Tab item="access">"Access"</Part>
                </Part>

                <Part part=tabs::Part::Panel item="fruit">
                    <h2 id="fruit-value">"Fruit: " {move || fruit.get().join(", ")}</h2>
                    <Root machine=Listbox::default() label="Fruit" on_value_change=on_fruit>
                        <Part part=listbox::Part::Root>
                            <Part part=listbox::Part::Option item="apple">
                                "Apple"
                            </Part>
                            <Part part=listbox::Part::Option item="banana" disabled=true>
                                "Banana"
                            </Part>
                            <Part part=listbox::Part::Option item="blueberry">
                                "Blueberry"
                            </Part>
                            <Part part=listbox::Part::Option item="cherry">
                                "Cherry"
                            </Part>
                        </Part>
                    </Root>
                </Part>

                <Part part=tabs::Part::Panel item="access">
                    <h2 id="access-value">"Access: " {move || access.get().join(", ")}</h2>
                    <p>"Agents may not grant delete."</p>
                    <Root machine=Listbox::new(multiple) label="Access" on_value_change=on_access>
                        <Part part=listbox::Part::Root>
                            <Part part=listbox::Part::Option item="read">
                                "Read"
                            </Part>
                            <Part part=listbox::Part::Option item="write">
                                "Write"
                            </Part>
                            <Part part=listbox::Part::Option item="share">
                                "Share"
                            </Part>
                            <Part part=listbox::Part::Option item="delete">
                                "Delete"
                            </Part>
                        </Part>
                    </Root>
                </Part>
            </Root>
        </main>
    }
}

fn main() {
    agent::install();
    mount_to_body(App);
}
