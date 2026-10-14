//! Integration between Dioxus and Blitz
use crate::events::{
    BlitzKeyboardData, NativeClickData, NativeConverter, NativeFocusData, NativeFormData,
    NativeMountedData, NativePointerData,
};
use crate::mounted::RequestQueue;
use crate::mutation_writer::{DioxusState, MutationWriter};
use crate::qual_name;
use crate::NodeId;
use blitz_dom::{
    Attribute, BaseDocument, Document, DocumentConfig, EventDriver, EventHandler, Node, DEFAULT_CSS,
};
use blitz_traits::events::{BlitzMouseButtonEvent, DomEvent, DomEventData, EventState, UiEvent};
use dioxus_core::{ElementId, Event, VirtualDom};
use dioxus_html::{set_event_converter, PlatformEventData};
use futures_util::task::noop_waker;
use futures_util::{pin_mut, FutureExt};
use std::ops::{Deref, DerefMut};
use std::sync::LazyLock;
use std::task::{Context as TaskContext, Waker};
use std::{any::Any, rc::Rc};

fn wrap_event_data<T: Any>(value: T) -> Rc<dyn Any> {
    Rc::new(PlatformEventData::new(Box::new(value)))
}

/// Get the value of the "dioxus-data-id" attribute parsed aa usize
fn get_dioxus_id(node: &Node) -> Option<ElementId> {
    node.element_data()?
        .attrs
        .iter()
        .find(|attr| *attr.name.local == *"data-dioxus-id")
        .and_then(|attr| attr.value.parse::<usize>().ok())
        .map(ElementId)
}

/// Integrates [`BaseDocument`] from  [`blitz-dom`](blitz_dom)  with [`VirtualDom`] from [`dioxus-core`](dioxus_core)
///
/// ### Example
///
/// ```rust
/// use blitz_traits::shell::{Viewport, ColorScheme};
/// use dioxus_native_dom::{DioxusDocument, DocumentConfig};
/// use dioxus::prelude::*;
///
/// // Example Dioxus app
/// fn app() -> Element {
///     rsx! {
///         div { "Hello, world!" }
///     }
/// }
///
/// fn main() {
///    let vdom = VirtualDom::new(app);
///    let mut doc = DioxusDocument::new(vdom, DocumentConfig {
///         viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
///         ..Default::default()
///    });
///    doc.initial_build();
/// }
/// ```
///
/// You can just push events into the [`DioxusDocument`] with [`doc.handle_ui_event(..)`](Self::handle_ui_event)
/// and then flush the changes with [`doc.poll(..)`](Self::poll)
pub struct DioxusDocument {
    pub inner: BaseDocument,
    pub vdom: VirtualDom,
    pub vdom_state: DioxusState,

    #[allow(unused)]
    pub(crate) html_element_id: NodeId,
    #[allow(unused)]
    pub(crate) head_element_id: NodeId,
    #[allow(unused)]
    pub(crate) body_element_id: NodeId,
    #[allow(unused)]
    pub(crate) main_element_id: NodeId,

    /// PATCH(rust-ui): queries from `MountedData` handles, answered in `poll`
    pub(crate) mounted_requests: RequestQueue,
    /// PATCH(rust-ui): the focused node the vdom was last told about (for focus/blur events)
    pub(crate) last_focus: Option<NodeId>,
}

impl DioxusDocument {
    /// Create a new [`DioxusDocument`] from a [`VirtualDom`].
    pub fn new(vdom: VirtualDom, mut config: DocumentConfig) -> Self {
        // Only really needs to happen once
        set_event_converter(Box::new(NativeConverter {}));

        config.base_url = Some(
            config
                .base_url
                .unwrap_or_else(|| String::from("dioxus://index.html")),
        );
        let mut doc = BaseDocument::new(config);

        // Include default stylesheet
        doc.add_user_agent_stylesheet(DEFAULT_CSS);

        // Create some minimal HTML to render the app into.
        // HTML is equivalent to:
        //
        // <html>
        // <head></head>
        // <body>
        //    <div id="main"></div>
        // </body>
        // </html>
        //
        // TODO: Support arbitrary "index.html" templates

        // Create the html element
        let mut mutr = doc.mutate();
        let html_element_id = mutr.create_element(qual_name("html", None), vec![]);
        mutr.append_children(mutr.doc.root_node().id, &[html_element_id]);

        // Create the body element
        let head_element_id = mutr.create_element(qual_name("head", None), vec![]);
        mutr.append_children(html_element_id, &[head_element_id]);

        // Create the body element
        let body_element_id = mutr.create_element(qual_name("body", None), vec![]);
        mutr.append_children(html_element_id, &[body_element_id]);

        // Create another virtual element to hold the root <div id="main"></div> under the html element
        let main_attr = blitz_dom::Attribute {
            name: qual_name("id", None),
            value: "main".to_string(),
        };
        let main_element_id = mutr.create_element(qual_name("main", None), vec![main_attr]);
        mutr.append_children(body_element_id, &[main_element_id]);

        drop(mutr);

        let vdom_state = DioxusState::create(main_element_id);
        Self {
            vdom,
            vdom_state,
            inner: doc,
            html_element_id,
            head_element_id,
            body_element_id,
            main_element_id,
            mounted_requests: Default::default(),
            last_focus: None,
        }
    }

    /// Run an initial build of the Dioxus vdom
    pub fn initial_build(&mut self) {
        let mut writer = MutationWriter::new(&mut self.inner, &mut self.vdom_state);
        self.vdom.rebuild(&mut writer);
        drop(writer);
        self.fire_mounted();
    }

    /// PATCH(rust-ui): fire `onmounted` for elements created since the last call.
    fn fire_mounted(&mut self) {
        let queue = std::mem::take(&mut self.vdom_state.mounted_queue);
        for id in queue {
            let Some(node_id) = self.vdom_state.try_element_to_node_id(id) else {
                continue;
            };
            let data = wrap_event_data(NativeMountedData {
                node_id,
                requests: self.mounted_requests.clone(),
            });
            self.vdom
                .runtime()
                .handle_event("mounted", Event::new(data, false), id);
        }
    }

    /// PATCH(rust-ui): answer queued `MountedData` queries, then report focus changes.
    /// Returns whether anything was answered.
    fn service_mounted_requests(&mut self) -> bool {
        let requests = std::mem::take(&mut *self.mounted_requests.borrow_mut());
        if requests.is_empty() {
            return false;
        }
        self.inner.resolve_now();
        if crate::mounted::service(&mut self.inner, requests) {
            self.sync_focus();
        }
        true
    }

    /// PATCH(rust-ui): fire blur/focusout and focus/focusin when Blitz's focus moved.
    fn sync_focus(&mut self) {
        let now = self.inner.focus_node_id();
        let before = std::mem::replace(&mut self.last_focus, now);
        if now == before {
            return;
        }
        let data = wrap_event_data(NativeFocusData {});
        if let Some(old) = before.filter(|&id| self.inner.get_node(id).is_some()) {
            let chain = self.inner.node_chain(old);
            dispatch(&self.vdom, &self.inner, "blur", &data, false, &chain[..1]);
            dispatch(&self.vdom, &self.inner, "focusout", &data, true, &chain);
        }
        if let Some(new) = now {
            let chain = self.inner.node_chain(new);
            dispatch(&self.vdom, &self.inner, "focus", &data, false, &chain[..1]);
            dispatch(&self.vdom, &self.inner, "focusin", &data, true, &chain);
        }
    }

    /// Used to respond to a `CreateHeadElement` event generated by Dioxus. These
    /// events allow Dioxus to create elements in the `<head>` of the document.
    #[doc(hidden)]
    pub fn create_head_element(
        &mut self,
        name: &str,
        attributes: &[(String, String)],
        contents: &Option<String>,
    ) {
        let mut mutr = self.inner.mutate();

        let attributes = attributes
            .iter()
            .map(|(name, value)| Attribute {
                name: qual_name(name, None),
                value: value.clone(),
            })
            .collect();

        let new_elem_id = mutr.create_element(qual_name(name, None), attributes);
        mutr.append_children(self.head_element_id, &[new_elem_id]);
        if let Some(contents) = contents {
            let text_node_id = mutr.create_text_node(contents);
            mutr.append_children(new_elem_id, &[text_node_id]);
        }
    }
}

// Implement DocumentLike and required traits for DioxusDocument
impl Document for DioxusDocument {
    fn id(&self) -> usize {
        self.inner.id()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn poll(&mut self, cx: Option<TaskContext>) -> bool {
        // PATCH(rust-ui): answer element queries made since the last poll.
        let answered = self.service_mounted_requests();
        let ready = {
            let fut = self.vdom.wait_for_work();
            pin_mut!(fut);

            static NOOP_WAKER: LazyLock<Waker> = LazyLock::new(noop_waker);
            let mut cx = cx.unwrap_or_else(|| TaskContext::from_waker(&NOOP_WAKER));
            fut.poll_unpin(&mut cx).is_ready()
        };
        if !ready {
            // Polling tasks may have queued element queries without dirtying anything.
            return self.service_mounted_requests() || answered;
        }

        let mut writer = MutationWriter::new(&mut self.inner, &mut self.vdom_state);
        self.vdom.render_immediate(&mut writer);
        drop(writer);
        self.fire_mounted();
        self.service_mounted_requests();

        true
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let handler = DioxusEventHandler {
            vdom: &mut self.vdom,
            vdom_state: &mut self.vdom_state,
        };
        let mut driver = EventDriver::new(self.inner.mutate(), handler);
        driver.handle_ui_event(event);
        drop(driver);
        self.sync_focus();
    }
}

impl Deref for DioxusDocument {
    type Target = BaseDocument;
    fn deref(&self) -> &BaseDocument {
        &self.inner
    }
}
impl DerefMut for DioxusDocument {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl From<DioxusDocument> for BaseDocument {
    fn from(doc: DioxusDocument) -> BaseDocument {
        doc.inner
    }
}

pub struct DioxusEventHandler<'v> {
    vdom: &'v mut VirtualDom,
    #[allow(dead_code, reason = "WIP")]
    vdom_state: &'v mut DioxusState,
}

/// PATCH(rust-ui): deliver one event to the vdom. Dioxus bubbles by itself, so the event is
/// handed to the innermost element of `chain` that has listeners, once. (Upstream handed it to
/// every element in the chain, so a bubbling handler on an ancestor could run several times.)
/// A non-bubbling event only reaches `chain[0]`.
fn dispatch(
    vdom: &VirtualDom,
    doc: &BaseDocument,
    name: &'static str,
    data: &Rc<dyn Any>,
    bubbles: bool,
    chain: &[usize],
) -> Option<Event<dyn Any>> {
    let chain = if bubbles {
        chain
    } else {
        &chain[..chain.len().min(1)]
    };
    let id = chain
        .iter()
        .find_map(|&node_id| doc.get_node(node_id).and_then(get_dioxus_id))?;
    let event = Event::new(data.clone(), bubbles);
    vdom.runtime().handle_event(name, event.clone(), id);
    Some(event)
}

impl DioxusEventHandler<'_> {
    fn mouse_data(doc: &BaseDocument, target: usize, m: &BlitzMouseButtonEvent) -> NativeClickData {
        let scroll = doc.viewport_scroll();
        let zoom = doc.viewport().zoom() as f64;
        let page = (m.x as f64, m.y as f64);
        let client = (page.0 - scroll.x / zoom, page.1 - scroll.y / zoom);
        let origin = doc
            .get_node(target)
            .map(|n| n.border_box_position())
            .unwrap_or_default();
        let element = (page.0 - origin.x as f64, page.1 - origin.y as f64);
        NativeClickData {
            event: m.clone(),
            client,
            page,
            element,
        }
    }

    /// Fire out/leave on what the pointer left and over/enter on what it entered.
    fn update_hover(&mut self, doc: &BaseDocument, chain: &[usize], data: &NativeClickData) {
        let old = std::mem::replace(&mut self.vdom_state.hover_chain, chain.to_vec());
        if old.first() == chain.first() {
            return;
        }
        let shared = old
            .iter()
            .rev()
            .zip(chain.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let left = &old[..old.len() - shared];
        let entered = &chain[..chain.len() - shared];
        let mouse = wrap_event_data(data.clone());
        let pointer = wrap_event_data(NativePointerData(data.clone()));
        let has = |s: &DioxusState, n: &str| s.has_listeners(n);
        let live = |id: &&usize| doc.get_node(**id).is_some();

        if !old.is_empty() {
            for (name, data) in [("pointerout", &pointer), ("mouseout", &mouse)] {
                if has(self.vdom_state, name) {
                    dispatch(self.vdom, doc, name, data, true, &old);
                }
            }
            for (name, data) in [("pointerleave", &pointer), ("mouseleave", &mouse)] {
                if has(self.vdom_state, name) {
                    for &id in left.iter().filter(live) {
                        dispatch(self.vdom, doc, name, data, false, &[id]);
                    }
                }
            }
        }
        if !chain.is_empty() {
            for (name, data) in [("pointerover", &pointer), ("mouseover", &mouse)] {
                if has(self.vdom_state, name) {
                    dispatch(self.vdom, doc, name, data, true, chain);
                }
            }
            for (name, data) in [("pointerenter", &pointer), ("mouseenter", &mouse)] {
                if has(self.vdom_state, name) {
                    for &id in entered.iter().rev() {
                        dispatch(self.vdom, doc, name, data, false, &[id]);
                    }
                }
            }
        }
    }
}

impl EventHandler for DioxusEventHandler<'_> {
    fn handle_event(
        &mut self,
        chain: &[usize],
        event: &mut DomEvent,
        mutr: &mut blitz_dom::DocumentMutator<'_>,
        event_state: &mut EventState,
    ) {
        let doc = &*mutr.doc;
        let mut fire = |s: &mut Self, name: &'static str, data: Rc<dyn Any>, bubbles: bool| {
            if !s.vdom_state.has_listeners(name) {
                return;
            }
            let Some(dx_event) = dispatch(s.vdom, doc, name, &data, bubbles, chain) else {
                return;
            };
            if !dx_event.default_action_enabled() {
                event_state.prevent_default();
            }
            if !dx_event.propagates() {
                event_state.stop_propagation();
            }
        };

        let name = event.name();
        let bubbles = event.bubbles;
        match &event.data {
            DomEventData::MouseMove(m) | DomEventData::MouseDown(m) | DomEventData::MouseUp(m) => {
                let data = Self::mouse_data(doc, event.target, m);
                let pointer_name = match &event.data {
                    DomEventData::MouseMove(_) => {
                        self.update_hover(doc, chain, &data);
                        "pointermove"
                    }
                    DomEventData::MouseDown(_) => "pointerdown",
                    _ => "pointerup",
                };
                let pointer = wrap_event_data(NativePointerData(data.clone()));
                fire(self, pointer_name, pointer, true);
                fire(self, name, wrap_event_data(data), bubbles);
            }
            DomEventData::Click(m) => {
                let data = Self::mouse_data(doc, event.target, m);
                fire(self, name, wrap_event_data(data), bubbles);
            }

            DomEventData::KeyDown(kevent)
            | DomEventData::KeyUp(kevent)
            | DomEventData::KeyPress(kevent) => {
                let data = wrap_event_data(BlitzKeyboardData(kevent.clone()));
                fire(self, name, data, bubbles);
            }

            DomEventData::Input(data) => {
                let data = wrap_event_data(NativeFormData {
                    value: data.value.clone(),
                    values: vec![],
                });
                fire(self, name, data, bubbles);
            }

            // TODO: Implement IME handling
            DomEventData::Ime(_) => {}
        }
    }
}
