use blitz_traits::events::{BlitzKeyEvent, BlitzMouseButtonEvent, MouseEventButton};
use dioxus_html::{
    geometry::{ClientPoint, ElementPoint, PagePoint, ScreenPoint},
    input_data::{MouseButton, MouseButtonSet},
    point_interaction::{
        InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction,
    },
    AnimationData, CancelData, ClipboardData, CompositionData, DragData, FocusData, FormData,
    FormValue, HasFileData, HasFocusData, HasFormData, HasKeyboardData, HasMouseData,
    HasPointerData, HtmlEventConverter, ImageData, KeyboardData, MediaData, MountedData, MouseData,
    PlatformEventData, PointerData, ResizeData, ScrollData, SelectionData, ToggleData, TouchData,
    TransitionData, VisibleData, WheelData,
};
use keyboard_types::{Code, Key, Location, Modifiers};
use std::any::Any;

pub(crate) use crate::mounted::NativeMountedData;

pub struct NativeConverter {}

impl HtmlEventConverter for NativeConverter {
    fn convert_cancel_data(&self, _event: &PlatformEventData) -> CancelData {
        unimplemented!("todo: convert_cancel_data in dioxus-native. requires support in blitz")
    }

    fn convert_form_data(&self, event: &PlatformEventData) -> FormData {
        event.downcast::<NativeFormData>().unwrap().clone().into()
    }

    fn convert_mouse_data(&self, event: &PlatformEventData) -> MouseData {
        if let Some(pointer) = event.downcast::<NativePointerData>() {
            return pointer.0.clone().into();
        }
        event.downcast::<NativeClickData>().unwrap().clone().into()
    }

    fn convert_keyboard_data(&self, event: &PlatformEventData) -> KeyboardData {
        event
            .downcast::<BlitzKeyboardData>()
            .unwrap()
            .clone()
            .into()
    }

    fn convert_focus_data(&self, _event: &PlatformEventData) -> FocusData {
        NativeFocusData {}.into()
    }

    fn convert_animation_data(&self, _event: &PlatformEventData) -> AnimationData {
        unimplemented!("todo: convert_animation_data in dioxus-native. requires support in blitz")
    }

    fn convert_clipboard_data(&self, _event: &PlatformEventData) -> ClipboardData {
        unimplemented!("todo: convert_clipboard_data in dioxus-native. requires support in blitz")
    }

    fn convert_composition_data(&self, _event: &PlatformEventData) -> CompositionData {
        unimplemented!("todo: convert_composition_data in dioxus-native. requires support in blitz")
    }

    fn convert_drag_data(&self, _event: &PlatformEventData) -> DragData {
        unimplemented!("todo: convert_drag_data in dioxus-native. requires support in blitz")
    }

    // PATCH(rust-ui): `onload` on images (upstream: `unimplemented!`).
    fn convert_image_data(&self, event: &PlatformEventData) -> ImageData {
        ImageData::new(event.downcast::<NativeImageData>().unwrap().clone())
    }

    fn convert_media_data(&self, _event: &PlatformEventData) -> MediaData {
        unimplemented!("todo: convert_media_data in dioxus-native. requires support in blitz")
    }

    // PATCH(rust-ui): onmounted and pointer events (upstream: `unimplemented!`).
    fn convert_mounted_data(&self, event: &PlatformEventData) -> MountedData {
        MountedData::new(event.downcast::<NativeMountedData>().unwrap().clone())
    }

    fn convert_pointer_data(&self, event: &PlatformEventData) -> PointerData {
        if let Some(pointer) = event.downcast::<NativePointerData>() {
            return pointer.clone().into();
        }
        NativePointerData(event.downcast::<NativeClickData>().unwrap().clone()).into()
    }

    fn convert_scroll_data(&self, _event: &PlatformEventData) -> ScrollData {
        unimplemented!("todo: convert_scroll_data in dioxus-native. requires support in blitz")
    }

    fn convert_selection_data(&self, _event: &PlatformEventData) -> SelectionData {
        unimplemented!("todo: convert_selection_data in dioxus-native. requires support in blitz")
    }

    fn convert_toggle_data(&self, _event: &PlatformEventData) -> ToggleData {
        unimplemented!("todo: convert_toggle_data in dioxus-native. requires support in blitz")
    }

    fn convert_touch_data(&self, _event: &PlatformEventData) -> TouchData {
        unimplemented!("todo: convert_touch_data in dioxus-native. requires support in blitz")
    }

    fn convert_transition_data(&self, _event: &PlatformEventData) -> TransitionData {
        unimplemented!("todo: convert_transition_data in dioxus-native. requires support in blitz")
    }

    fn convert_wheel_data(&self, _event: &PlatformEventData) -> WheelData {
        unimplemented!("todo: convert_wheel_data in dioxus-native. requires support in blitz")
    }

    fn convert_resize_data(&self, _event: &PlatformEventData) -> ResizeData {
        unimplemented!("todo: convert_resize_data in dioxus-native. requires support in blitz")
    }

    fn convert_visible_data(&self, _event: &PlatformEventData) -> VisibleData {
        unimplemented!("todo: convert_visible_data in dioxus-native. requires support in blitz")
    }
}

#[derive(Clone, Debug)]
pub struct NativeFormData {
    pub value: String,
    pub values: Vec<(String, FormValue)>,
}

impl HasFormData for NativeFormData {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }

    fn value(&self) -> String {
        self.value.clone()
    }

    fn values(&self) -> Vec<(String, FormValue)> {
        self.values.clone()
    }
    fn valid(&self) -> bool {
        // todo: actually implement validation here.
        true
    }
}

impl HasFileData for NativeFormData {
    fn files(&self) -> Vec<dioxus_html::FileData> {
        vec![]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BlitzKeyboardData(pub(crate) BlitzKeyEvent);

impl ModifiersInteraction for BlitzKeyboardData {
    fn modifiers(&self) -> Modifiers {
        self.0.modifiers
    }
}

impl HasKeyboardData for BlitzKeyboardData {
    fn key(&self) -> Key {
        self.0.key.clone()
    }

    fn code(&self) -> Code {
        self.0.code
    }

    fn location(&self) -> Location {
        self.0.location
    }

    fn is_auto_repeating(&self) -> bool {
        self.0.is_auto_repeating
    }

    fn is_composing(&self) -> bool {
        self.0.is_composing
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn Any
    }
}

/// PATCH(rust-ui): mouse data with real client, page and element coordinates (upstream
/// reported page coordinates as client ones and panicked on the others).
#[derive(Clone)]
pub struct NativeClickData {
    pub(crate) event: BlitzMouseButtonEvent,
    /// Relative to the window (page minus viewport scroll).
    pub(crate) client: (f64, f64),
    /// Relative to the document.
    pub(crate) page: (f64, f64),
    /// Relative to the target element's border box.
    pub(crate) element: (f64, f64),
}

impl InteractionLocation for NativeClickData {
    fn client_coordinates(&self) -> ClientPoint {
        ClientPoint::new(self.client.0, self.client.1)
    }

    // The window position is not known, so screen coordinates are client coordinates.
    fn screen_coordinates(&self) -> ScreenPoint {
        ScreenPoint::new(self.client.0, self.client.1)
    }

    fn page_coordinates(&self) -> PagePoint {
        PagePoint::new(self.page.0, self.page.1)
    }
}

impl InteractionElementOffset for NativeClickData {
    fn element_coordinates(&self) -> ElementPoint {
        ElementPoint::new(self.element.0, self.element.1)
    }
}

impl ModifiersInteraction for NativeClickData {
    fn modifiers(&self) -> Modifiers {
        self.event.mods
    }
}

impl PointerInteraction for NativeClickData {
    fn trigger_button(&self) -> Option<MouseButton> {
        Some(match self.event.button {
            MouseEventButton::Main => MouseButton::Primary,
            MouseEventButton::Auxiliary => MouseButton::Auxiliary,
            MouseEventButton::Secondary => MouseButton::Secondary,
            MouseEventButton::Fourth => MouseButton::Fourth,
            MouseEventButton::Fifth => MouseButton::Fifth,
        })
    }

    fn held_buttons(&self) -> MouseButtonSet {
        dioxus_html::input_data::decode_mouse_button_set(self.event.buttons.bits() as u16)
    }
}
impl HasMouseData for NativeClickData {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }
}

#[derive(Clone)]
pub struct NativeFocusData {}
impl HasFocusData for NativeFocusData {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }
}

/// PATCH(rust-ui): pointer events, synthesised from Blitz's mouse events (one mouse pointer).
#[derive(Clone)]
pub struct NativePointerData(pub(crate) NativeClickData);

impl InteractionLocation for NativePointerData {
    fn client_coordinates(&self) -> ClientPoint {
        self.0.client_coordinates()
    }
    fn screen_coordinates(&self) -> ScreenPoint {
        self.0.screen_coordinates()
    }
    fn page_coordinates(&self) -> PagePoint {
        self.0.page_coordinates()
    }
}

impl InteractionElementOffset for NativePointerData {
    fn element_coordinates(&self) -> ElementPoint {
        self.0.element_coordinates()
    }
}

impl ModifiersInteraction for NativePointerData {
    fn modifiers(&self) -> Modifiers {
        self.0.modifiers()
    }
}

impl PointerInteraction for NativePointerData {
    fn trigger_button(&self) -> Option<MouseButton> {
        self.0.trigger_button()
    }
    fn held_buttons(&self) -> MouseButtonSet {
        self.0.held_buttons()
    }
}

impl HasPointerData for NativePointerData {
    fn pointer_id(&self) -> i32 {
        1
    }
    fn width(&self) -> f64 {
        1.0
    }
    fn height(&self) -> f64 {
        1.0
    }
    fn pressure(&self) -> f32 {
        if self.0.event.buttons.is_empty() {
            0.0
        } else {
            0.5
        }
    }
    fn tangential_pressure(&self) -> f32 {
        0.0
    }
    fn tilt_x(&self) -> i32 {
        0
    }
    fn tilt_y(&self) -> i32 {
        0
    }
    fn twist(&self) -> i32 {
        0
    }
    fn pointer_type(&self) -> String {
        "mouse".into()
    }
    fn is_primary(&self) -> bool {
        true
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }
}

impl HasMouseData for NativePointerData {
    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }
}

/// PATCH(rust-ui): data for an image `load` (or `error`) event.
#[derive(Clone, Debug)]
pub(crate) struct NativeImageData {
    pub(crate) error: bool,
}

impl dioxus_html::HasImageData for NativeImageData {
    fn load_error(&self) -> bool {
        self.error
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }
}
