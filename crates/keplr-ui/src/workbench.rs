use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

const LAYOUT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Files,
    Editor,
    Outline,
    Search,
    SourceControl,
    Problems,
    Tasks,
    Console,
    Terminal,
    Serial,
    Chat,
}

impl PaneKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Editor => "Editor",
            Self::Outline => "Outline",
            Self::Search => "Search",
            Self::SourceControl => "Source Control",
            Self::Problems => "Problems",
            Self::Tasks => "Tasks",
            Self::Console => "Console",
            Self::Terminal => "Terminal",
            Self::Serial => "Serial",
            Self::Chat => "Chat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneCard {
    pub id: String,
    pub kind: PaneKind,
    pub title: String,
    pub resource: Option<String>,
}

impl PaneCard {
    pub fn new(id: impl Into<String>, kind: PaneKind, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            resource: None,
        }
    }

    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneLeaf {
    pub id: String,
    pub cards: Vec<PaneCard>,
    pub active_card: Option<String>,
    pub visible: bool,
}

impl PaneLeaf {
    fn new(id: impl Into<String>, cards: Vec<PaneCard>, visible: bool) -> Self {
        let active_card = cards.first().map(|card| card.id.clone());
        Self {
            id: id.into(),
            cards,
            active_card,
            visible,
        }
    }

    fn normalize_active(&mut self) {
        if self
            .active_card
            .as_ref()
            .is_some_and(|active| self.cards.iter().any(|card| &card.id == active))
        {
            return;
        }
        self.active_card = self.cards.first().map(|card| card.id.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaneNode {
    Leaf(PaneLeaf),
    Split {
        id: String,
        axis: PaneAxis,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneTree {
    pub root: PaneNode,
}

impl PaneTree {
    pub fn new(root: PaneNode) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &PaneNode {
        &self.root
    }

    pub fn root_mut(&mut self) -> &mut PaneNode {
        &mut self.root
    }

    pub fn leaf_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        collect_leaf_ids(&self.root, &mut ids);
        ids
    }

    pub fn leaves(&self) -> Vec<&PaneLeaf> {
        let mut leaves = Vec::new();
        collect_leaves(&self.root, &mut leaves);
        leaves
    }

    pub fn leaf(&self, id: &str) -> Option<&PaneLeaf> {
        find_leaf(&self.root, id)
    }

    pub fn leaf_mut(&mut self, id: &str) -> Option<&mut PaneLeaf> {
        find_leaf_mut(&mut self.root, id)
    }

    pub fn find_card(&self, id: &str) -> Option<&PaneCard> {
        fn visit<'a>(node: &'a PaneNode, id: &str) -> Option<&'a PaneCard> {
            match node {
                PaneNode::Leaf(leaf) => leaf.cards.iter().find(|card| card.id == id),
                PaneNode::Split { first, second, .. } => {
                    visit(first, id).or_else(|| visit(second, id))
                }
            }
        }
        visit(&self.root, id)
    }

    pub fn split_leaf(
        &mut self,
        leaf_id: &str,
        axis: PaneAxis,
        card: PaneCard,
    ) -> Result<String, LayoutError> {
        if self.leaf(leaf_id).is_none() {
            return Err(LayoutError::UnknownLeaf(leaf_id.to_string()));
        }
        let new_id = self.unique_leaf_id(leaf_id, &card.id);
        let replacement = replace_leaf(&self.root, leaf_id, axis, card, &new_id);
        self.root = replacement.ok_or_else(|| LayoutError::UnknownLeaf(leaf_id.to_string()))?;
        Ok(new_id)
    }

    pub fn move_card(&mut self, card_id: &str, target_leaf: &str) -> Result<(), LayoutError> {
        if self.leaf(target_leaf).is_none() {
            return Err(LayoutError::UnknownLeaf(target_leaf.to_string()));
        }
        let source_leaf = self
            .leaf_ids()
            .into_iter()
            .find(|id| {
                self.leaf(id)
                    .is_some_and(|leaf| leaf.cards.iter().any(|card| card.id == card_id))
            })
            .ok_or_else(|| LayoutError::UnknownCard(card_id.to_string()))?;
        if source_leaf == target_leaf {
            return Err(LayoutError::SameLeaf);
        }
        let card = take_card(&mut self.root, card_id)
            .ok_or_else(|| LayoutError::UnknownCard(card_id.to_string()))?;
        let target = self
            .leaf_mut(target_leaf)
            .ok_or_else(|| LayoutError::UnknownLeaf(target_leaf.to_string()))?;
        target.cards.push(card);
        target.active_card = Some(
            target
                .cards
                .last()
                .map(|card| card.id.clone())
                .unwrap_or_default(),
        );
        target.visible = true;
        target.normalize_active();
        Ok(())
    }

    pub fn close_card(&mut self, card_id: &str) -> Result<(), LayoutError> {
        if !self.find_card(card_id).is_some() {
            return Err(LayoutError::UnknownCard(card_id.to_string()));
        }
        remove_card(&mut self.root, card_id);
        Ok(())
    }

    pub fn set_leaf_visible(&mut self, leaf_id: &str, visible: bool) -> Result<(), LayoutError> {
        let leaf = self
            .leaf_mut(leaf_id)
            .ok_or_else(|| LayoutError::UnknownLeaf(leaf_id.to_string()))?;
        leaf.visible = visible;
        Ok(())
    }

    pub fn set_ratio(&mut self, split_id: &str, ratio: f32) -> Result<(), LayoutError> {
        if !(0.05..=0.95).contains(&ratio) {
            return Err(LayoutError::InvalidRatio(ratio));
        }
        let mut found = false;
        set_split_ratio(&mut self.root, split_id, ratio, &mut found);
        if found {
            Ok(())
        } else {
            Err(LayoutError::UnknownSplit(split_id.to_string()))
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(value: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(value)
    }

    fn unique_leaf_id(&self, leaf_id: &str, card_id: &str) -> String {
        let base = format!("{leaf_id}-{card_id}");
        let mut candidate = base.clone();
        let mut suffix = 2;
        while self.leaf(&candidate).is_some() {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        candidate
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkbenchLayout {
    pub version: u32,
    pub tree: PaneTree,
    pub focused_leaf: String,
}

impl WorkbenchLayout {
    pub fn current_default() -> Self {
        let left = PaneLeaf::new(
            "left",
            vec![
                PaneCard::new("files-default", PaneKind::Files, "Files"),
                PaneCard::new("search-default", PaneKind::Search, "Search"),
                PaneCard::new("source-default", PaneKind::SourceControl, "Source Control"),
                PaneCard::new("outline-default", PaneKind::Outline, "Outline"),
            ],
            true,
        );
        let editor = PaneLeaf::new(
            "editor",
            vec![PaneCard::new("editor-default", PaneKind::Editor, "Editor")],
            true,
        );
        let right = PaneLeaf::new(
            "right",
            vec![PaneCard::new(
                "symbols-default",
                PaneKind::Outline,
                "Symbols",
            )],
            false,
        );
        let bottom = PaneLeaf::new(
            "bottom",
            vec![
                PaneCard::new("terminal-default", PaneKind::Terminal, "Terminal"),
                PaneCard::new("problems-default", PaneKind::Problems, "Problems"),
                PaneCard::new("tasks-default", PaneKind::Tasks, "Tasks"),
                PaneCard::new("console-default", PaneKind::Console, "Console"),
                PaneCard::new("serial-default", PaneKind::Serial, "Serial"),
            ],
            false,
        );
        let top = PaneNode::Split {
            id: "split-top".to_string(),
            axis: PaneAxis::Horizontal,
            ratio: 0.22,
            first: Box::new(PaneNode::Leaf(left)),
            second: Box::new(PaneNode::Split {
                id: "split-center".to_string(),
                axis: PaneAxis::Horizontal,
                ratio: 0.76,
                first: Box::new(PaneNode::Leaf(editor)),
                second: Box::new(PaneNode::Leaf(right)),
            }),
        };
        let root = PaneNode::Split {
            id: "split-root".to_string(),
            axis: PaneAxis::Vertical,
            ratio: 0.78,
            first: Box::new(top),
            second: Box::new(PaneNode::Leaf(bottom)),
        };
        Self {
            version: LAYOUT_VERSION,
            tree: PaneTree::new(root),
            focused_leaf: "editor".to_string(),
        }
    }

    pub fn focus(&mut self, leaf_id: &str) -> Result<(), LayoutError> {
        if self.tree.leaf(leaf_id).is_none() {
            return Err(LayoutError::UnknownLeaf(leaf_id.to_string()));
        }
        self.focused_leaf = leaf_id.to_string();
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(value: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutError {
    UnknownLeaf(String),
    UnknownCard(String),
    UnknownSplit(String),
    SameLeaf,
    InvalidRatio(f32),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLeaf(id) => write!(f, "unknown pane leaf: {id}"),
            Self::UnknownCard(id) => write!(f, "unknown pane card: {id}"),
            Self::UnknownSplit(id) => write!(f, "unknown pane split: {id}"),
            Self::SameLeaf => write!(f, "card is already in the target pane"),
            Self::InvalidRatio(ratio) => {
                write!(f, "pane ratio must be between 0.05 and 0.95: {ratio}")
            }
        }
    }
}

impl Error for LayoutError {}

fn collect_leaf_ids(node: &PaneNode, ids: &mut Vec<String>) {
    match node {
        PaneNode::Leaf(leaf) => ids.push(leaf.id.clone()),
        PaneNode::Split { first, second, .. } => {
            collect_leaf_ids(first, ids);
            collect_leaf_ids(second, ids);
        }
    }
}

fn collect_leaves<'a>(node: &'a PaneNode, leaves: &mut Vec<&'a PaneLeaf>) {
    match node {
        PaneNode::Leaf(leaf) => leaves.push(leaf),
        PaneNode::Split { first, second, .. } => {
            collect_leaves(first, leaves);
            collect_leaves(second, leaves);
        }
    }
}

fn find_leaf<'a>(node: &'a PaneNode, id: &str) -> Option<&'a PaneLeaf> {
    match node {
        PaneNode::Leaf(leaf) if leaf.id == id => Some(leaf),
        PaneNode::Leaf(_) => None,
        PaneNode::Split { first, second, .. } => {
            find_leaf(first, id).or_else(|| find_leaf(second, id))
        }
    }
}

fn find_leaf_mut<'a>(node: &'a mut PaneNode, id: &str) -> Option<&'a mut PaneLeaf> {
    match node {
        PaneNode::Leaf(leaf) if leaf.id == id => Some(leaf),
        PaneNode::Leaf(_) => None,
        PaneNode::Split { first, second, .. } => {
            find_leaf_mut(first, id).or_else(|| find_leaf_mut(second, id))
        }
    }
}

fn replace_leaf(
    node: &PaneNode,
    leaf_id: &str,
    axis: PaneAxis,
    card: PaneCard,
    new_leaf_id: &str,
) -> Option<PaneNode> {
    match node {
        PaneNode::Leaf(leaf) if leaf.id == leaf_id => Some(PaneNode::Split {
            id: format!("split-{new_leaf_id}"),
            axis,
            ratio: 0.5,
            first: Box::new(node.clone()),
            second: Box::new(PaneNode::Leaf(PaneLeaf::new(new_leaf_id, vec![card], true))),
        }),
        PaneNode::Leaf(_) => None,
        PaneNode::Split {
            id,
            axis: parent_axis,
            ratio,
            first,
            second,
        } => {
            let first_replacement = replace_leaf(first, leaf_id, axis, card.clone(), new_leaf_id);
            if let Some(first_replacement) = first_replacement {
                return Some(PaneNode::Split {
                    id: id.clone(),
                    axis: *parent_axis,
                    ratio: *ratio,
                    first: Box::new(first_replacement),
                    second: second.clone(),
                });
            }
            let second_replacement = replace_leaf(second, leaf_id, axis, card, new_leaf_id);
            second_replacement.map(|replacement| PaneNode::Split {
                id: id.clone(),
                axis: *parent_axis,
                ratio: *ratio,
                first: first.clone(),
                second: Box::new(replacement),
            })
        }
    }
}

fn take_card(node: &mut PaneNode, card_id: &str) -> Option<PaneCard> {
    match node {
        PaneNode::Leaf(leaf) => {
            let index = leaf.cards.iter().position(|card| card.id == card_id)?;
            let card = leaf.cards.remove(index);
            leaf.normalize_active();
            Some(card)
        }
        PaneNode::Split { first, second, .. } => {
            take_card(first, card_id).or_else(|| take_card(second, card_id))
        }
    }
}

fn remove_card(node: &mut PaneNode, card_id: &str) -> bool {
    match node {
        PaneNode::Leaf(leaf) => {
            let before = leaf.cards.len();
            leaf.cards.retain(|card| card.id != card_id);
            let removed = leaf.cards.len() != before;
            if removed {
                leaf.normalize_active();
            }
            removed
        }
        PaneNode::Split { first, second, .. } => {
            remove_card(first, card_id) || remove_card(second, card_id)
        }
    }
}

fn set_split_ratio(node: &mut PaneNode, split_id: &str, ratio: f32, found: &mut bool) {
    match node {
        PaneNode::Leaf(_) => {}
        PaneNode::Split {
            id,
            ratio: current,
            first,
            second,
            ..
        } => {
            if id == split_id {
                *current = ratio;
                *found = true;
                return;
            }
            set_split_ratio(first, split_id, ratio, found);
            set_split_ratio(second, split_id, ratio, found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_unique_cards() {
        let layout = WorkbenchLayout::current_default();
        let mut ids = Vec::new();
        for leaf in layout.tree.leaves() {
            for card in &leaf.cards {
                assert!(!ids.contains(&card.id));
                ids.push(card.id.clone());
            }
        }
        assert_eq!(ids.len(), 11);
    }

    #[test]
    fn ratios_are_bounded() {
        let mut layout = WorkbenchLayout::current_default();
        assert!(matches!(
            layout.tree.set_ratio("split-root", 0.99),
            Err(LayoutError::InvalidRatio(_))
        ));
        assert!(layout.tree.set_ratio("split-root", 0.6).is_ok());
    }
}
