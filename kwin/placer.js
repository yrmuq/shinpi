// Copyright (C) 2026 yrmuq, GPL-3.0-or-later.
const GAP = 8;
const windows = new Map();
let lastAnchor = null;

function textOf(value) {
    return value == null ? "" : String(value);
}

function isOurs(window) {
    const classes = (textOf(window.resourceClass) + " " + textOf(window.resourceName)).split(/\s+/);
    return !window.dialog && (classes.includes("shinpi") || classes.includes("shinpi-popup"));
}

function captionOf(window) {
    return textOf(window.captionNormal || window.caption);
}

function isPopup(window) {
    return isOurs(window) && captionOf(window) === "shinpi-devices";
}

function isNotice(window) {
    return isOurs(window) && captionOf(window) === "shinpi-notices";
}

function visibleNotices() {
    return workspace.windowList().filter((window) => isNotice(window) && !window.deleted && !window.hidden);
}

function repositionNotices() {
    const popup = workspace.windowList().find((window) => isPopup(window) && !window.deleted && !window.hidden);
    if (!popup) { return; }
    const pf = popup.frameGeometry;
    for (const notice of visibleNotices()) {
        const entry = trackedNotices.get(notice);
        if (entry && entry.moving) { continue; }
        const nf = notice.frameGeometry;
        const ratio = ratioOf(notice);
        const target = { x: snap(pf.x, ratio), y: snap(pf.y - nf.height, ratio), width: pf.width, height: nf.height };
        if (Math.abs(nf.x - target.x) >= 0.5 || Math.abs(nf.y - target.y) >= 0.5 || Math.abs(nf.width - target.width) >= 0.5) {
            if (entry) { entry.moving = true; }
            try { notice.frameGeometry = target; }
            finally { if (entry) { entry.moving = false; } }
        }
    }
}

function hideFromTaskbar(window) {
    window.skipTaskbar = true;
    window.skipSwitcher = true;
    window.skipPager = true;
}

function contains(rect, point) {
    return (
        point.x >= rect.x && point.x < rect.x + rect.width &&
        point.y >= rect.y && point.y < rect.y + rect.height
    );
}

function edgeOf(panel, screen) {
    if (panel.width >= panel.height) {
        return Math.abs(panel.y - screen.y) < Math.abs(screen.y + screen.height - panel.y - panel.height)
            ? "top" : "bottom";
    }
    return Math.abs(panel.x - screen.x) < Math.abs(screen.x + screen.width - panel.x - panel.width)
        ? "left" : "right";
}

function clamp(value, start, end) {
    return Math.max(start, Math.min(value, Math.max(start, end)));
}

function ratioOf(window) {
    const output = window && window.output;
    const ratio = output && output.devicePixelRatio;
    return ratio > 0 ? ratio : 1;
}

function snap(value, ratio) {
    return Math.round(value * ratio) / ratio;
}

function placement(size, area, anchor) {
    let x = area.x + area.width - size.width - GAP;
    let y = area.y + GAP;
    if (anchor) {
        const panel = anchor.panel.frameGeometry;
        switch (anchor.edge) {
        case "top":
            x = anchor.point.x - size.width / 2;
            y = panel.y + panel.height + GAP;
            break;
        case "bottom":
            x = anchor.point.x - size.width / 2;
            y = panel.y - size.height - GAP;
            break;
        case "left":
            x = panel.x + panel.width + GAP;
            y = anchor.point.y - size.height / 2;
            break;
        case "right":
            x = panel.x - size.width - GAP;
            y = anchor.point.y - size.height / 2;
            break;
        }
    }
    return {
        x: Math.round(clamp(x, area.x + GAP, area.x + area.width - size.width - GAP)),
        y: Math.round(clamp(y, area.y + GAP, area.y + area.height - size.height - GAP)),
        width: size.width,
        height: size.height,
    };
}

function panelWindows() {
    return workspace.windowList().filter((window) =>
        window.dock && String(window.resourceClass) === "plasmashell" &&
        window.frameGeometry.width > 0 && window.frameGeometry.height > 0
    );
}

function selectAnchor(window) {
    const panels = panelWindows();
    const cursor = { x: workspace.cursorPos.x, y: workspace.cursorPos.y };
    const clicked = panels.find((panel) => contains(panel.frameGeometry, cursor));
    if (clicked) {
        lastAnchor = {
            panel: clicked,
            edge: edgeOf(clicked.frameGeometry, clicked.output.geometry),
            point: cursor,
        };
        return lastAnchor;
    }
    if (lastAnchor && panels.includes(lastAnchor.panel)) {
        const panel = lastAnchor.panel;
        const edge = edgeOf(panel.frameGeometry, panel.output.geometry);
        if (edge === lastAnchor.edge && contains(panel.frameGeometry, lastAnchor.point)) {
            return lastAnchor;
        }
    }
    lastAnchor = null;
    const output = workspace.screenAt(cursor) || window.output;
    const panel = panels.find((candidate) => candidate.output === output);
    if (!panel) {
        return null;
    }
    const rect = panel.frameGeometry;
    return {
        panel: panel,
        edge: edgeOf(rect, panel.output.geometry),
        point: { x: rect.x + rect.width - GAP, y: rect.y + rect.height - GAP },
    };
}

function place(entry) {
    const window = entry.window;
    if (entry.moving || window.deleted || window.hidden || window.fullScreen || window.move || window.resize) {
        return;
    }
    const frame = window.frameGeometry;
    if (frame.width <= 0 || frame.height <= 0) {
        return;
    }
    const output = entry.anchor ? entry.anchor.panel.output : window.output;
    const area = workspace.clientArea(KWin.MaximizeArea, output, workspace.currentDesktop);
    const ratio = ratioOf(window);
    const target = placement(frame, area, entry.anchor);
    target.x = snap(target.x, ratio);
    const noticeHeight = Math.max(0, ...visibleNotices().map((notice) => notice.frameGeometry.height));
    target.y = snap(Math.max(target.y, area.y + GAP + noticeHeight), ratio);
    if (Math.abs(frame.x - target.x) < 0.5 && Math.abs(frame.y - target.y) < 0.5) {
        repositionNotices();
        return;
    }
    entry.moving = true;
    try {
        window.frameGeometry = target;
    } finally {
        entry.moving = false;
    }
    repositionNotices();
}

function track(window) {
    if (!isPopup(window) || windows.has(window)) {
        return;
    }
    hideFromTaskbar(window);
    window.keepAbove = true;
    const entry = { window: window, anchor: null, moving: false };
    windows.set(window, entry);
    const onShow = () => {
        hideFromTaskbar(window);
        entry.anchor = selectAnchor(window);
        place(entry);
    };
    const visibilityChanged = window.windowShown || window.hiddenChanged;
    if (visibilityChanged) {
        visibilityChanged.connect(() => {
            if (!window.hidden) {
                onShow();
            }
        });
    }
    window.frameGeometryChanged.connect(() => place(entry));
    window.closed.connect(() => windows.delete(window));
    onShow();
}

const trackedNotices = new Map();
function trackNotice(window) {
    if (!isNotice(window) || trackedNotices.has(window)) { return; }
    const entry = { moving: false };
    trackedNotices.set(window, entry);
    hideFromTaskbar(window);
    window.keepAbove = true;
    const update = () => {
        if (entry.moving) { return; }
        hideFromTaskbar(window);
        for (const entry of windows.values()) { place(entry); }
        repositionNotices();
    };
    window.frameGeometryChanged.connect(update);
    const visibilityChanged = window.windowShown || window.hiddenChanged;
    if (visibilityChanged) { visibilityChanged.connect(update); }
    window.closed.connect(() => trackedNotices.delete(window));
    update();
}

function arm(window) {
    const identify = () => {
        if (isOurs(window) && String(window.resourceClass) === "shinpi-popup") {
            hideFromTaskbar(window);
        }
        track(window);
        trackNotice(window);
    };
    identify();
    if (window.captionChanged) { window.captionChanged.connect(identify); }
    if (window.captionNormalChanged) { window.captionNormalChanged.connect(identify); }
    if (window.windowClassChanged) { window.windowClassChanged.connect(identify); }
}
workspace.windowAdded.connect(arm);
workspace.windowList().forEach(arm);
