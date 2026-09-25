import test from "node:test";
import assert from "node:assert/strict";
import {
  LAYOUT_VERSION,
  currentLayout,
  leafIds,
  splitLeaf,
  moveCard,
  focusLeaf,
  addCard,
  setLeafVisible,
  serialize,
  deserialize,
} from "../src/ui_layout.js";

test("current layout preserves the existing workbench", () => {
  const layout = currentLayout();
  assert.equal(layout.version, LAYOUT_VERSION);
  assert.deepEqual(leafIds(layout), ["left", "editor", "right", "bottom"]);
  assert.equal(layout.tree.leaves.left.visible, true);
  assert.equal(layout.tree.leaves.right.visible, false);
  assert.equal(layout.tree.leaves.bottom.visible, false);
});

test("splitting and moving cards preserves the rest of the graph", () => {
  const first = currentLayout();
  const split = splitLeaf(first, "editor", "horizontal", {
    id: "terminal-1",
    kind: "terminal",
    title: "Terminal",
    resource: null,
  });
  assert.notEqual(split.leafId, "editor");
  assert.equal(split.layout.tree.leaves.editor.cards[0].id, "editor-default");
  assert.equal(split.layout.tree.leaves[split.leafId].cards[0].id, "terminal-1");

  const moved = moveCard(split.layout, "terminal-1", "right");
  assert.equal(moved.tree.leaves.editor.cards.some(card => card.id === "terminal-1"), false);
  assert.equal(moved.tree.leaves.right.visible, true);
  assert.equal(moved.tree.leaves.right.activeCard, "terminal-1");
  assert.equal(first.tree.leaves.editor.cards.length, 1);
});

test("layout serialization round trips and focus/visibility are explicit", () => {
  const layout = currentLayout();
  const next = focusLeaf(setLeafVisible(layout, "bottom", true), "bottom");
  const decoded = deserialize(serialize(next));
  assert.deepEqual(decoded, next);
  assert.equal(decoded.focusedLeaf, "bottom");
  assert.equal(decoded.tree.leaves.bottom.visible, true);
});

test("cards can be added to the focused leaf and become visible", () => {
  const layout = addCard(currentLayout(), "editor", {
    id: "chat-1",
    kind: "chat",
    title: "Chat",
    resource: null,
  });
  assert.equal(layout.tree.leaves.editor.visible, true);
  assert.equal(layout.tree.leaves.editor.activeCard, "chat-1");
  assert.equal(layout.tree.leaves.editor.cards.at(-1).id, "chat-1");
});

test("invalid layout operations fail without mutating the source", () => {
  const layout = currentLayout();
  assert.throws(() => moveCard(layout, "missing", "right"), /unknown card/);
  assert.throws(() => focusLeaf(layout, "missing"), /unknown leaf/);
  assert.throws(() => setLeafVisible(layout, "missing", true), /unknown leaf/);
  assert.equal(layout.tree.leaves.right.visible, false);
});
