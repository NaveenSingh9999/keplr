export const LAYOUT_VERSION = 1;

const clone = value => JSON.parse(JSON.stringify(value));
const fail = message => { throw new Error(message); };

function card(id, kind, title, resource = null) {
  return { id, kind, title, resource };
}

function leaf(id, cards, visible) {
  return {
    type: "leaf",
    id,
    cards,
    activeCard: cards[0]?.id ?? null,
    visible,
  };
}

function split(id, axis, ratio, first, second) {
  return { type: "split", id, axis, ratio, first, second };
}

function indexLeaves(root) {
  const leaves = {};
  const visit = node => {
    if (node.type === "leaf") {
      leaves[node.id] = node;
      return;
    }
    visit(node.first);
    visit(node.second);
  };
  visit(root);
  return leaves;
}

function makeTree(root) {
  return { root, leaves: indexLeaves(root) };
}

function currentRoot() {
  const left = leaf("left", [
    card("files-default", "files", "Files"),
    card("search-default", "search", "Search"),
    card("source-default", "source-control", "Source Control"),
    card("outline-default", "outline", "Outline"),
  ], true);
  const editor = leaf("editor", [card("editor-default", "editor", "Editor")], true);
  const right = leaf("right", [card("symbols-default", "outline", "Symbols")], false);
  const bottom = leaf("bottom", [
    card("terminal-default", "terminal", "Terminal"),
    card("problems-default", "problems", "Problems"),
    card("tasks-default", "tasks", "Tasks"),
    card("console-default", "console", "Console"),
    card("serial-default", "serial", "Serial"),
  ], false);
  return split(
    "split-root",
    "vertical",
    0.78,
    split(
      "split-top",
      "horizontal",
      0.22,
      left,
      split("split-center", "horizontal", 0.76, editor, right)
    ),
    bottom
  );
}

export function currentLayout() {
  return {
    version: LAYOUT_VERSION,
    tree: makeTree(currentRoot()),
    focusedLeaf: "editor",
  };
}

export function leafIds(layout) {
  return Object.keys(layout.tree.leaves);
}

export function getLeaf(layout, id) {
  return layout.tree.leaves[id] ?? null;
}

export function getCard(layout, id) {
  for (const leaf of Object.values(layout.tree.leaves)) {
    const found = leaf.cards.find(item => item.id === id);
    if (found) return found;
  }
  return null;
}

function replaceLeaf(node, leafId, axis, newCard, newLeafId) {
  if (node.type === "leaf") {
    if (node.id !== leafId) return null;
    return split(`split-${newLeafId}`, axis, 0.5, node, leaf(newLeafId, [newCard], true));
  }
  const first = replaceLeaf(node.first, leafId, axis, newCard, newLeafId);
  if (first) return { ...node, first };
  const second = replaceLeaf(node.second, leafId, axis, newCard, newLeafId);
  return second ? { ...node, second } : null;
}

function uniqueLeafId(layout, leafId, cardId) {
  const base = `${leafId}-${cardId}`;
  let candidate = base;
  let suffix = 2;
  while (layout.tree.leaves[candidate]) {
    candidate = `${base}-${suffix}`;
    suffix += 1;
  }
  return candidate;
}

export function splitLeaf(layout, leafId, axis, newCard) {
  if (!layout.tree.leaves[leafId]) fail(`unknown leaf: ${leafId}`);
  if (!['horizontal', 'vertical'].includes(axis)) fail(`unknown split axis: ${axis}`);
  if (getCard(layout, newCard.id)) fail(`duplicate card: ${newCard.id}`);
  const next = clone(layout);
  const newLeafId = uniqueLeafId(layout, leafId, newCard.id);
  const root = replaceLeaf(next.tree.root, leafId, axis, clone(newCard), newLeafId);
  if (!root) fail(`unknown leaf: ${leafId}`);
  next.tree = makeTree(root);
  return { layout: next, leafId: newLeafId };
}

function removeCard(node, cardId) {
  if (node.type === "leaf") {
    const index = node.cards.findIndex(item => item.id === cardId);
    if (index < 0) return null;
    const [removed] = node.cards.splice(index, 1);
    if (node.activeCard === cardId) node.activeCard = node.cards[0]?.id ?? null;
    return removed;
  }
  const first = removeCard(node.first, cardId);
  if (first) return first;
  return removeCard(node.second, cardId);
}

function findLeafForCard(node, cardId) {
  if (node.type === "leaf") return node.cards.some(item => item.id === cardId) ? node.id : null;
  return findLeafForCard(node.first, cardId) || findLeafForCard(node.second, cardId);
}

export function moveCard(layout, cardId, targetLeafId) {
  if (!layout.tree.leaves[targetLeafId]) fail(`unknown leaf: ${targetLeafId}`);
  const sourceLeafId = findLeafForCard(layout.tree.root, cardId);
  if (!sourceLeafId) fail(`unknown card: ${cardId}`);
  if (sourceLeafId === targetLeafId) fail("card is already in the target leaf");
  const next = clone(layout);
  const moved = removeCard(next.tree.root, cardId);
  if (!moved) fail(`unknown card: ${cardId}`);
  next.tree = makeTree(next.tree.root);
  const target = next.tree.leaves[targetLeafId];
  target.cards.push(moved);
  target.activeCard = moved.id;
  target.visible = true;
  next.tree = makeTree(next.tree.root);
  return next;
}

export function removeCardFromLayout(layout, cardId) {
  if (!getCard(layout, cardId)) fail(`unknown card: ${cardId}`);
  const next = clone(layout);
  if (!removeCard(next.tree.root, cardId)) fail(`unknown card: ${cardId}`);
  next.tree = makeTree(next.tree.root);
  return next;
}

export function addCard(layout, leafId, newCard) {
  if (!layout.tree.leaves[leafId]) fail(`unknown leaf: ${leafId}`);
  if (getCard(layout, newCard.id)) fail(`duplicate card: ${newCard.id}`);
  const next = clone(layout);
  next.tree = makeTree(next.tree.root);
  next.tree.leaves[leafId].cards.push(clone(newCard));
  next.tree.leaves[leafId].activeCard = newCard.id;
  next.tree.leaves[leafId].visible = true;
  next.tree = makeTree(next.tree.root);
  next.focusedLeaf = leafId;
  return next;
}

export function setCardActive(layout, leafId, cardId) {
  const leaf = layout.tree.leaves[leafId];
  if (!leaf) fail(`unknown leaf: ${leafId}`);
  if (!leaf.cards.some(card => card.id === cardId)) fail(`unknown card: ${cardId}`);
  const next = clone(layout);
  next.tree = makeTree(next.tree.root);
  next.tree.leaves[leafId].activeCard = cardId;
  next.tree = makeTree(next.tree.root);
  next.focusedLeaf = leafId;
  return next;
}

export function setLeafVisible(layout, leafId, visible) {
  if (!layout.tree.leaves[leafId]) fail(`unknown leaf: ${leafId}`);
  const next = clone(layout);
  next.tree = makeTree(next.tree.root);
  next.tree.leaves[leafId].visible = Boolean(visible);
  next.tree = makeTree(next.tree.root);
  return next;
}

export function focusLeaf(layout, leafId) {
  if (!layout.tree.leaves[leafId]) fail(`unknown leaf: ${leafId}`);
  const next = clone(layout);
  next.focusedLeaf = leafId;
  return next;
}

export function setSplitRatio(layout, splitId, ratio) {
  if (!Number.isFinite(ratio) || ratio < 0.05 || ratio > 0.95) {
    fail(`invalid split ratio: ${ratio}`);
  }
  let found = false;
  const visit = node => {
    if (node.type !== "split") return;
    if (node.id === splitId) {
      node.ratio = ratio;
      found = true;
      return;
    }
    visit(node.first);
    visit(node.second);
  };
  const next = clone(layout);
  visit(next.tree.root);
  if (!found) fail(`unknown split: ${splitId}`);
  return next;
}

export function serialize(layout) {
  return JSON.stringify(layout);
}

export function deserialize(value) {
  const layout = typeof value === "string" ? JSON.parse(value) : clone(value);
  if (!layout || layout.version !== LAYOUT_VERSION || !layout.tree?.root || !layout.tree?.leaves) {
    fail("invalid workbench layout");
  }
  const leaves = indexLeaves(layout.tree.root);
  if (Object.keys(leaves).length !== Object.keys(layout.tree.leaves).length) {
    fail("invalid workbench leaf index");
  }
  for (const [id, leaf] of Object.entries(leaves)) {
    if (layout.tree.leaves[id]?.id !== id) fail("invalid workbench leaf index");
    if (leaf.activeCard && !leaf.cards.some(card => card.id === leaf.activeCard)) {
      fail("invalid active card");
    }
  }
  if (!leaves[layout.focusedLeaf]) fail("invalid focused leaf");
  return layout;
}

export function migrateLegacyLayout(legacy) {
  const next = currentLayout();
  if (!legacy || typeof legacy !== "object") return next;
  if (legacy.docks) {
    for (const [id, visible] of Object.entries(legacy.docks)) {
      if (id in next.tree.leaves) next.tree.leaves[id].visible = Boolean(visible);
    }
  }
  if (legacy.focusedLeaf) {
    if (!next.tree.leaves[legacy.focusedLeaf]) fail("invalid focused leaf");
    next.focusedLeaf = legacy.focusedLeaf;
  }
  return next;
}
