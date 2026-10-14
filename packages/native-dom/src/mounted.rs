//! PATCH(rust-ui): `onmounted` support. Upstream answered every mounted event with
//! `unimplemented!`.
//!
//! The element handle cannot borrow the document, so each query is queued and answered by
//! [`crate::DioxusDocument`] the next time it is polled or handles an event
//! (`service_mounted_requests`), with layout brought up to date first.

use dioxus_html::geometry::{
    euclid::{Point2D, Rect, Size2D, Vector2D},
    PixelsRect, PixelsSize, PixelsVector2D,
};
use dioxus_html::{
    MountedError, MountedResult, RenderedElementBacking, ScrollBehavior, ScrollLogicalPosition,
    ScrollToOptions,
};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// A one-shot reply slot (a tiny stand-in for a oneshot channel; no extra dependency).
pub(crate) struct Reply<T>(Rc<RefCell<(Option<T>, Option<Waker>)>>);

impl<T> Clone for Reply<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Reply<T> {
    fn new() -> Self {
        Self(Rc::new(RefCell::new((None, None))))
    }

    pub(crate) fn send(&self, value: T) {
        let mut slot = self.0.borrow_mut();
        slot.0 = Some(value);
        if let Some(waker) = slot.1.take() {
            waker.wake();
        }
    }
}

impl<T> Future for Reply<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut slot = self.0.borrow_mut();
        match slot.0.take() {
            Some(value) => Poll::Ready(value),
            None => {
                slot.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

pub(crate) enum MountedRequest {
    ClientRect(usize, Reply<MountedResult<PixelsRect>>),
    ScrollOffset(usize, Reply<MountedResult<PixelsVector2D>>),
    ScrollSize(usize, Reply<MountedResult<PixelsSize>>),
    ScrollIntoView(usize, ScrollToOptions, Reply<MountedResult<()>>),
    ScrollBy(usize, PixelsVector2D, Reply<MountedResult<()>>),
    Focus(usize, bool, Reply<MountedResult<()>>),
}

pub(crate) type RequestQueue = Rc<RefCell<Vec<MountedRequest>>>;

#[derive(Clone)]
pub(crate) struct NativeMountedData {
    pub(crate) node_id: usize,
    pub(crate) requests: RequestQueue,
}

impl NativeMountedData {
    fn ask<T: 'static>(
        &self,
        make: impl FnOnce(usize, Reply<T>) -> MountedRequest,
    ) -> Pin<Box<dyn Future<Output = T>>> {
        let reply = Reply::new();
        self.requests
            .borrow_mut()
            .push(make(self.node_id, reply.clone()));
        Box::pin(reply)
    }
}

impl RenderedElementBacking for NativeMountedData {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_scroll_offset(&self) -> Pin<Box<dyn Future<Output = MountedResult<PixelsVector2D>>>> {
        self.ask(MountedRequest::ScrollOffset)
    }

    fn get_scroll_size(&self) -> Pin<Box<dyn Future<Output = MountedResult<PixelsSize>>>> {
        self.ask(MountedRequest::ScrollSize)
    }

    fn get_client_rect(&self) -> Pin<Box<dyn Future<Output = MountedResult<PixelsRect>>>> {
        self.ask(MountedRequest::ClientRect)
    }

    fn scroll_to(
        &self,
        options: ScrollToOptions,
    ) -> Pin<Box<dyn Future<Output = MountedResult<()>>>> {
        self.ask(|id, reply| MountedRequest::ScrollIntoView(id, options, reply))
    }

    fn scroll(
        &self,
        coordinates: PixelsVector2D,
        _behavior: ScrollBehavior,
    ) -> Pin<Box<dyn Future<Output = MountedResult<()>>>> {
        self.ask(|id, reply| MountedRequest::ScrollBy(id, coordinates, reply))
    }

    fn set_focus(&self, focus: bool) -> Pin<Box<dyn Future<Output = MountedResult<()>>>> {
        self.ask(|id, reply| MountedRequest::Focus(id, focus, reply))
    }
}

fn gone<T>() -> MountedResult<T> {
    Err(MountedError::OperationFailed(Box::new(
        std::io::Error::other("element is no longer in the document"),
    )))
}

/// Answer every queued request against the (freshly laid out) document.
/// Returns whether focus may have changed.
pub(crate) fn service(doc: &mut blitz_dom::BaseDocument, requests: Vec<MountedRequest>) -> bool {
    let mut focus_changed = false;
    let scroll = doc.viewport_scroll();
    let exists = |doc: &blitz_dom::BaseDocument, id: usize| {
        doc.get_node(id).is_some_and(|n| n.parent.is_some())
    };
    for request in requests {
        match request {
            MountedRequest::ClientRect(id, reply) => reply.send(if exists(doc, id) {
                let node = doc.get_node(id).unwrap();
                let pos = node.border_box_position();
                let size = node.final_layout.size;
                Ok(Rect::new(
                    Point2D::new(pos.x as f64 - scroll.x, pos.y as f64 - scroll.y),
                    Size2D::new(size.width as f64, size.height as f64),
                ))
            } else {
                gone()
            }),
            MountedRequest::ScrollOffset(id, reply) => reply.send(if exists(doc, id) {
                let o = doc.get_node(id).unwrap().scroll_offset;
                Ok(Vector2D::new(o.x, o.y))
            } else {
                gone()
            }),
            MountedRequest::ScrollSize(id, reply) => reply.send(if exists(doc, id) {
                let l = doc.get_node(id).unwrap().final_layout;
                Ok(Size2D::new(
                    l.content_size.width.max(l.size.width) as f64,
                    l.content_size.height.max(l.size.height) as f64,
                ))
            } else {
                gone()
            }),
            MountedRequest::ScrollBy(id, by, reply) => reply.send(if exists(doc, id) {
                doc.scroll_node_by(id, -by.x, -by.y);
                Ok(())
            } else {
                gone()
            }),
            MountedRequest::ScrollIntoView(id, options, reply) => reply.send(if exists(doc, id) {
                scroll_into_view(doc, id, &options);
                Ok(())
            } else {
                gone()
            }),
            MountedRequest::Focus(id, focus, reply) => reply.send(if exists(doc, id) {
                if focus {
                    doc.set_focus_to(id);
                } else if doc.get_focussed_node_id() == Some(id) {
                    doc.clear_focus();
                }
                focus_changed = true;
                Ok(())
            } else {
                gone()
            }),
        }
    }
    focus_changed
}

/// Scroll each scrollable ancestor (and then the viewport) so the element is in view.
fn scroll_into_view(doc: &mut blitz_dom::BaseDocument, id: usize, options: &ScrollToOptions) {
    fn delta(
        start: f64,
        len: f64,
        view_start: f64,
        view_len: f64,
        at: ScrollLogicalPosition,
    ) -> f64 {
        let end = start + len;
        let view_end = view_start + view_len;
        match at {
            ScrollLogicalPosition::Start => start - view_start,
            ScrollLogicalPosition::End => end - view_end,
            ScrollLogicalPosition::Center => (start + len / 2.0) - (view_start + view_len / 2.0),
            ScrollLogicalPosition::Nearest => {
                if start < view_start {
                    start - view_start
                } else if end > view_end {
                    (end - view_end).min(start - view_start)
                } else {
                    0.0
                }
            }
        }
    }

    let mut ancestor = doc.get_node(id).and_then(|n| n.layout_parent.get());
    while let Some(a) = ancestor {
        let node = doc.get_node(a).unwrap();
        if node.is_scroll_container()
            && !node
                .data
                .is_element_with_tag_name(&blitz_dom::local_name!("html"))
        {
            let target = doc.get_node(id).unwrap();
            let t = target.border_box_position();
            let tsize = target.final_layout.size;
            let v = node.border_box_position();
            let vsize = node.final_layout.size;
            let dx = delta(
                t.x as f64,
                tsize.width as f64,
                v.x as f64,
                vsize.width as f64,
                clone_pos(&options.horizontal),
            );
            let dy = delta(
                t.y as f64,
                tsize.height as f64,
                v.y as f64,
                vsize.height as f64,
                clone_pos(&options.vertical),
            );
            if dx != 0.0 || dy != 0.0 {
                doc.scroll_node_by(a, -dx, -dy);
            }
        }
        ancestor = doc.get_node(a).unwrap().layout_parent.get();
    }

    let target = doc.get_node(id).unwrap();
    let t = target.border_box_position();
    let tsize = target.final_layout.size;
    let scroll = doc.viewport_scroll();
    let (w, h) = doc.viewport().window_size;
    let zoom = doc.viewport().scale_f64();
    let dx = delta(
        t.x as f64,
        tsize.width as f64,
        scroll.x,
        w as f64 / zoom,
        clone_pos(&options.horizontal),
    );
    let dy = delta(
        t.y as f64,
        tsize.height as f64,
        scroll.y,
        h as f64 / zoom,
        clone_pos(&options.vertical),
    );
    if dx != 0.0 || dy != 0.0 {
        doc.scroll_viewport_by(-dx, -dy);
    }
}

fn clone_pos(p: &ScrollLogicalPosition) -> ScrollLogicalPosition {
    match p {
        ScrollLogicalPosition::Start => ScrollLogicalPosition::Start,
        ScrollLogicalPosition::Center => ScrollLogicalPosition::Center,
        ScrollLogicalPosition::End => ScrollLogicalPosition::End,
        ScrollLogicalPosition::Nearest => ScrollLogicalPosition::Nearest,
    }
}
