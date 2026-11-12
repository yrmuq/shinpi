# Copyright (C) 2026 yrmuq
# SPDX-License-Identifier: GPL-3.0-or-later
"""Release notes from a dependency diff or from commit subjects."""

import re
import sys

SECTIONS = ("Added", "Changed", "Fixed", "Removed", "Security")


def deps_notes(diff):
    pairs = []
    path = None
    block = []

    def flush():
        if path is not None and path.endswith("Cargo.lock"):
            pairs.extend(_lock_pairs(block))
        elif path is not None and path.endswith((".yml", ".yaml")):
            pairs.extend(_action_pairs(_body(block, "-"), _body(block, "+")))
        elif path == "packaging/appimage/build.sh":
            pairs.extend(_tool_pairs(_body(block, "-"), _body(block, "+")))

    for line in diff.splitlines():
        if line.startswith("diff --git "):
            flush()
            found = re.search(r" b/(.+)$", line)
            path = found.group(1) if found else None
            block = []
            continue
        block.append(line)
    flush()
    if not pairs:
        return ""
    pairs = sorted(set(pairs))
    lines = ["Dependency updates."]
    for name, old, new in pairs:
        lines.append("- `%s` %s -> %s" % (name, old, new))
    return "\n".join(lines) + "\n"


def commit_notes(subjects):
    buckets = {name: [] for name in SECTIONS}
    for subject in subjects:
        section, text = _classify(subject)
        if section is None:
            continue
        buckets[section].append(text)
    lines = []
    for name in SECTIONS:
        if not buckets[name]:
            continue
        if lines:
            lines.append("")
        lines.append("### " + name)
        for text in buckets[name]:
            lines.append("- %s" % text)
    if not lines:
        return ""
    return "\n".join(lines) + "\n"


def _classify(subject):
    text = subject.strip()
    lowered = text.lower()
    if lowered.startswith("merge ") or lowered.startswith("version "):
        return None, None
    if re.search(r"chmod |\./shinpi|Test build", text):
        return None, None
    if not text.endswith("."):
        text = text + "."
    if re.match(r"^(add|implement)\b", lowered):
        return "Added", text
    if re.match(r"^(fix|resolve|prevent)\b", lowered):
        return "Fixed", text
    if re.match(r"^remove\b", lowered):
        return "Removed", text
    if re.match(r"^(redact|restrict)\b", lowered):
        return "Security", text
    return "Changed", text


def _body(lines, prefix):
    return [line[1:] for line in lines if line.startswith(prefix)]


def _lock_pairs(lines):
    pairs = []
    name = None
    old = None
    new = None

    def flush():
        nonlocal name, old, new
        if name and name != "shinpi" and old and new and old != new:
            pairs.append((name, old, new))
        name = None
        old = None
        new = None

    for line in lines:
        if not line or line.startswith(("---", "+++", "@@", "index ", "new file", "deleted file")):
            continue
        prefix = line[0]
        if prefix not in "+- ":
            continue
        body = line[1:].strip()
        if body == "[[package]]":
            flush()
            continue
        found = re.match(r'name = "([^"]+)"', body)
        if found:
            if name and found.group(1) != name:
                flush()
            name = found.group(1)
            continue
        found = re.match(r'version = "([^"]+)"', body)
        if found and name:
            if prefix == "-":
                old = found.group(1)
            elif prefix == "+":
                new = found.group(1)
    flush()
    return pairs


def _action_pairs(minus, plus):
    old = _actions(minus)
    new = _actions(plus)
    pairs = []
    for name in old:
        if name in new and old[name] != new[name]:
            pairs.append((name, old[name], new[name]))
    return pairs


def _actions(lines):
    found = {}
    pattern = re.compile(r"uses:\s*([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)@(\S+)")
    for line in lines:
        match = pattern.search(line)
        if match:
            found[match.group(1)] = match.group(2)
    return found


def _tool_pairs(minus, plus):
    old = _tool(minus)
    new = _tool(plus)
    if old and new and old != new:
        return [("appimagetool", old, new)]
    return []


def _tool(lines):
    pattern = re.compile(r"download/([^/\s\"]+)/appimagetool")
    for line in lines:
        match = pattern.search(line)
        if match:
            return match.group(1)
    return None


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in ("deps", "commits"):
        sys.exit(2)
    data = sys.stdin.read()
    if sys.argv[1] == "deps":
        text = deps_notes(data)
    else:
        text = commit_notes(data.splitlines())
    if not text:
        sys.exit(1)
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
