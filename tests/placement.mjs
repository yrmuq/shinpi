import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const context = vm.createContext({
    workspace: { windowList: () => [], windowAdded: { connect() {} } },
});
vm.runInContext(readFileSync(new URL('../kwin/placer.js', import.meta.url), 'utf8'), context);

test('popup stays beside all panel edges and inside the selected output', () => {
    const size = { width: 360, height: 140 };
    const area = { x: 100, y: 100, width: 1200, height: 800 };
    for (const [edge, panel, expected] of [
        ['bottom', { x: 100, y: 900, width: 1200, height: 40 }, [720, 752]],
        ['top', { x: 100, y: 60, width: 1200, height: 40 }, [720, 108]],
        ['left', { x: 60, y: 100, width: 40, height: 800 }, [108, 430]],
        ['right', { x: 1300, y: 100, width: 40, height: 800 }, [932, 430]],
    ]) {
        const anchor = { edge, panel: { frameGeometry: panel }, point: { x: 900, y: 500 } };
        const result = context.placement(size, area, anchor);
        assert.deepEqual([result.x, result.y], expected, edge);
        anchor.point = { x: 10000, y: 10000 };
        const clamped = context.placement(size, area, anchor);
        assert.ok(clamped.x >= area.x + 8 && clamped.x + size.width <= area.x + area.width - 8);
        assert.ok(clamped.y >= area.y + 8 && clamped.y + size.height <= area.y + area.height - 8);
    }
    assert.equal(context.snap(10, 1.25), 10.4);
    assert.equal(context.isPopup({ caption: 'shinpi-devices', resourceClass: 'shinpi-popup' }), true);
    assert.equal(context.isPopup({ caption: 'shinpi-devices', resourceClass: 'another-app' }), false);
});

test('a growing device list remains anchored to each panel edge', () => {
    const output = { geometry: { x: 0, y: 0, width: 1200, height: 800 }, devicePixelRatio: 1.25 };
    const area = { x: 40, y: 40, width: 1120, height: 720 };
    for (const panelRect of [
        { x: 0, y: 0, width: 1200, height: 40 },
        { x: 0, y: 760, width: 1200, height: 40 },
        { x: 0, y: 0, width: 40, height: 800 },
        { x: 1160, y: 0, width: 40, height: 800 },
    ]) {
        const panel = { dock: true, resourceClass: 'plasmashell', frameGeometry: panelRect, output };
        const point = { x: panelRect.x + panelRect.width / 2, y: panelRect.y + panelRect.height / 2 };
        let onResize;
        let frame = { x: 0, y: 0, width: 360, height: 140 };
        const popup = {
            caption: 'shinpi-devices', resourceClass: 'shinpi', output,
            frameGeometryChanged: { connect(callback) { onResize = callback; } },
            closed: { connect() {} },
            get frameGeometry() { return frame; },
            set frameGeometry(next) { frame = next; onResize?.(); },
        };
        Object.assign(context.workspace, {
            windowList: () => [panel, popup], cursorPos: point,
            screenAt: () => output, clientArea: () => area,
        });
        context.KWin = { MaximizeArea: 0 };
        context.track(popup);
        popup.frameGeometry = { ...frame, height: 300 };
        const expected = context.placement(frame, area, { panel, point, edge: context.edgeOf(panelRect, output.geometry) });
        assert.deepEqual([frame.x, frame.y], [context.snap(expected.x, 1.25), context.snap(expected.y, 1.25)]);
        assert.equal(popup.skipTaskbar, true);
    }
});


test('notification history follows popup width and stays above it at the top edge', () => {
    vm.runInContext('windows.clear(); trackedNotices.clear()', context);
    const output = { geometry: { x: 0, y: 0, width: 1200, height: 800 }, devicePixelRatio: 1 };
    const area = { x: 0, y: 40, width: 1200, height: 760 };
    const panel = { dock: true, resourceClass: 'plasmashell', frameGeometry: { x: 0, y: 0, width: 1200, height: 40 }, output };
    function surface(caption, initial) {
        let frame = initial;
        const callbacks = [];
        let visibility;
        return {
            caption, resourceClass: 'shinpi', output, hidden: false,
            closed: { connect() {} },
            hiddenChanged: { connect(callback) { visibility = callback; } },
            frameGeometryChanged: { connect(callback) { callbacks.push(callback); } },
            get frameGeometry() { return frame; },
            set frameGeometry(next) { frame = next; callbacks.forEach(callback => callback()); },
            visibility() { visibility?.(); },
        };
    }
    const popup = surface('shinpi-devices', { x: 0, y: 0, width: 360, height: 140 });
    const notes = surface('shinpi-notices', { x: 0, y: 0, width: 360, height: 232 });
    Object.assign(context.workspace, {
        windowList: () => [panel, popup, notes], cursorPos: { x: 600, y: 20 },
        screenAt: () => output, clientArea: () => area,
    });
    context.KWin = { MaximizeArea: 0 };
    context.track(popup);
    context.trackNotice(notes);
    assert.equal(notes.skipTaskbar, true);
    assert.ok(notes.frameGeometry.y >= area.y);
    assert.equal(notes.frameGeometry.y + notes.frameGeometry.height, popup.frameGeometry.y);
    popup.frameGeometry = { ...popup.frameGeometry, width: 520 };
    assert.equal(notes.frameGeometry.x, popup.frameGeometry.x);
    assert.equal(notes.frameGeometry.width, 520);
    notes.hidden = true;
    notes.visibility();
    assert.equal(popup.frameGeometry.y, 48);
    assert.equal(context.isNotice({ caption: 'shinpi-notices', resourceClass: 'other' }), false);
});
