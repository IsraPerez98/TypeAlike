//! The layout of the settings pages.
//!
//! It gives the stacks, the width of arranged views, the scroll views, and the rounded groups
//! with the separators between their rows. It also gives the spacing that the pages share.

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSLayoutAttribute, NSLayoutConstraint, NSLayoutConstraintOrientation,
    NSLayoutPriorityDefaultLow, NSResponder, NSScrollView, NSStackView, NSStackViewDistribution,
    NSStackViewGravity, NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSSize};

pub(super) const NO_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 0.0,
    left: 0.0,
    bottom: 0.0,
    right: 0.0,
};
const ROW_CONTAINER_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 0.0,
    left: 12.0,
    bottom: 0.0,
    right: 12.0,
};
pub(super) const LIST_PART_SPACING: CGFloat = 8.0;
const MIN_ROW_HEIGHT: CGFloat = 52.0;
const ROW_TEXT_PADDING: CGFloat = 8.0;
const ROW_COLUMN_SPACING: CGFloat = 12.0;
const ROW_TEXT_SPACING: CGFloat = 2.0;
const GROUP_CORNER_RADIUS: CGFloat = 10.0;
const GROUP_BORDER_WIDTH: CGFloat = 1.0;
pub(super) const ACCESSORY_SPACING: CGFloat = 8.0;

/// Adds a row of `texts`, then `controls`, left to right, at the bottom of the rounded group
/// `rows_stack`, with a separator above it. The row grows past its minimum height to keep a
/// padding around `texts` when they wrap. The first row of a group has no separator above it.
pub(super) fn add_grouped_row(
    rows_stack: &NSStackView,
    texts: &NSView,
    controls: &[&NSView],
    main_thread: MainThreadMarker,
) {
    let content = NSStackView::stackViewWithViews(&NSArray::from_slice(&[texts]), main_thread);
    for control in controls {
        content.addArrangedSubview(control);
    }
    content.setSpacing(ROW_COLUMN_SPACING);
    content.setDistribution(NSStackViewDistribution::Fill);
    // NSStackView does not keep its edge insets on the cross axis, so the padding is explicit.
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .heightAnchor()
            .constraintGreaterThanOrEqualToConstant(MIN_ROW_HEIGHT),
        texts
            .topAnchor()
            .constraintGreaterThanOrEqualToAnchor_constant(&content.topAnchor(), ROW_TEXT_PADDING),
        texts
            .bottomAnchor()
            .constraintLessThanOrEqualToAnchor_constant(&content.bottomAnchor(), -ROW_TEXT_PADDING),
    ]));
    let separator = separator(main_thread);
    let container = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        ROW_CONTAINER_INSETS,
        main_thread,
    );
    container.addArrangedSubview(&separator);
    container.addArrangedSubview(&content);
    rows_stack.addArrangedSubview(&container);
    fill_width(&container, rows_stack);
    fill_width(&separator, &container);
    fill_width(&content, &container);
    separator.setHidden(rows_stack.arrangedSubviews().count() == 1);
}

pub(super) fn scroll_view(content: &NSStackView, main_thread: MainThreadMarker) -> Retained<NSScrollView> {
    let document = FlippedView::new(main_thread);
    document.addSubview(content);
    let scroll_view = NSScrollView::new(main_thread);
    scroll_view.setHasVerticalScroller(true);
    scroll_view.setAutohidesScrollers(true);
    scroll_view.setDrawsBackground(false);
    scroll_view.setDocumentView(Some(&document));
    scroll_view.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Vertical);
    let clip_view = scroll_view.contentView();
    document.setTranslatesAutoresizingMaskIntoConstraints(false);
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        document
            .leadingAnchor()
            .constraintEqualToAnchor(&clip_view.leadingAnchor()),
        document
            .trailingAnchor()
            .constraintEqualToAnchor(&clip_view.trailingAnchor()),
        document
            .topAnchor()
            .constraintEqualToAnchor(&clip_view.topAnchor()),
        content
            .leadingAnchor()
            .constraintEqualToAnchor(&document.leadingAnchor()),
        content
            .trailingAnchor()
            .constraintEqualToAnchor(&document.trailingAnchor()),
        content.topAnchor().constraintEqualToAnchor(&document.topAnchor()),
        content
            .bottomAnchor()
            .constraintEqualToAnchor(&document.bottomAnchor()),
    ]));
    scroll_view
}

/// Wraps `content` in a rounded box like the grouped lists of System Settings.
pub(super) fn rounded_group(content: &NSView, main_thread: MainThreadMarker) -> Retained<NSBox> {
    let group = NSBox::new(main_thread);
    group.setBoxType(NSBoxType::Custom);
    group.setTitlePosition(NSTitlePosition::NoTitle);
    group.setContentViewMargins(NSSize::ZERO);
    group.setCornerRadius(GROUP_CORNER_RADIUS);
    group.setBorderWidth(GROUP_BORDER_WIDTH);
    group.setBorderColor(&NSColor::separatorColor());
    group.setFillColor(&NSColor::quaternarySystemFillColor());
    group.setContentView(Some(content));
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .leadingAnchor()
            .constraintEqualToAnchor(&group.leadingAnchor()),
        content
            .trailingAnchor()
            .constraintEqualToAnchor(&group.trailingAnchor()),
        content.topAnchor().constraintEqualToAnchor(&group.topAnchor()),
        content
            .bottomAnchor()
            .constraintEqualToAnchor(&group.bottomAnchor()),
    ]));
    group
}

fn separator(main_thread: MainThreadMarker) -> Retained<NSBox> {
    let separator = NSBox::new(main_thread);
    separator.setBoxType(NSBoxType::Separator);
    separator
}

/// Stacks `title` over `subtitle`, taking the free width of a row.
pub(super) fn row_texts(
    title: &NSView,
    subtitle: &NSView,
    main_thread: MainThreadMarker,
) -> Retained<NSStackView> {
    let texts = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        ROW_TEXT_SPACING,
        NO_INSETS,
        main_thread,
    );
    texts.addArrangedSubview(title);
    texts.addArrangedSubview(subtitle);
    texts.setContentHuggingPriority_forOrientation(
        NSLayoutPriorityDefaultLow,
        NSLayoutConstraintOrientation::Horizontal,
    );
    texts
}

pub(super) fn stack(
    orientation: NSUserInterfaceLayoutOrientation,
    spacing: CGFloat,
    insets: NSEdgeInsets,
    main_thread: MainThreadMarker,
) -> Retained<NSStackView> {
    let stack = NSStackView::new(main_thread);
    stack.setOrientation(orientation);
    let alignment = if orientation == NSUserInterfaceLayoutOrientation::Vertical {
        NSLayoutAttribute::Leading
    } else {
        NSLayoutAttribute::CenterY
    };
    stack.setAlignment(alignment);
    stack.setSpacing(spacing);
    stack.setEdgeInsets(insets);
    stack
}

/// Makes `view` as wide as the inside of the vertical `stack` it is arranged in.
pub(super) fn fill_width(view: &NSView, stack: &NSStackView) {
    let insets = stack.edgeInsets();
    view.widthAnchor()
        .constraintEqualToAnchor_constant(&stack.widthAnchor(), -(insets.left + insets.right))
        .setActive(true);
}

/// The layout of a page with a search field: the pinned `header`, a divider, and `content` that
/// scrolls.
pub(super) fn pinned_root(
    header: &NSView,
    content: &NSStackView,
    main_thread: MainThreadMarker,
) -> Retained<NSStackView> {
    let scroll_view = scroll_view(content, main_thread);
    let divider = separator(main_thread);
    let root = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        NO_INSETS,
        main_thread,
    );
    let root_views: [&NSView; 3] = [header, &divider, &scroll_view];
    for view in root_views {
        root.addArrangedSubview(view);
        fill_width(view, &root);
    }
    root.setDistribution(NSStackViewDistribution::Fill);
    root
}

/// Holds `content` below the toolbar, which covers the top of the panes of the window.
pub(super) fn below_toolbar(content: &NSView, main_thread: MainThreadMarker) -> Retained<NSView> {
    let container = NSView::new(main_thread);
    container.addSubview(content);
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .leadingAnchor()
            .constraintEqualToAnchor(&container.leadingAnchor()),
        content
            .trailingAnchor()
            .constraintEqualToAnchor(&container.trailingAnchor()),
        content
            .topAnchor()
            .constraintEqualToAnchor(&container.safeAreaLayoutGuide().topAnchor()),
        content
            .bottomAnchor()
            .constraintEqualToAnchor(&container.bottomAnchor()),
    ]));
    container
}

/// Returns a row that holds `view` at its trailing edge.
pub(super) fn trailing_row(view: &NSView, main_thread: MainThreadMarker) -> Retained<NSStackView> {
    let row = stack(
        NSUserInterfaceLayoutOrientation::Horizontal,
        0.0,
        NO_INSETS,
        main_thread,
    );
    row.addView_inGravity(view, NSStackViewGravity::Trailing);
    row
}

define_class!(
    // SAFETY: NSView has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GhostTypeFlippedView"]
    /// A plain view with a top-left origin, so scrolled content starts at the top.
    struct FlippedView;

    impl FlippedView {
        // SAFETY: matches the NSView method signature.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSView's init takes no arguments and returns an initialized view.
        unsafe { msg_send![super(this), init] }
    }
}
