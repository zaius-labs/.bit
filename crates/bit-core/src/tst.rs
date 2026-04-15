#[derive(Debug, PartialEq)]
pub enum TstResult {
    Exact,  // key exists verbatim — EpistemicState::Known
    Prefix, // key is a prefix of one or more stored keywords — EpistemicState::Unknown
    Miss,   // key shares no prefix with any stored keyword — EpistemicState::Invalid
}

struct TstNode {
    ch: char,
    is_end: bool,
    lo: Option<Box<TstNode>>,
    eq: Option<Box<TstNode>>,
    hi: Option<Box<TstNode>>,
}

pub struct Tst {
    root: Option<Box<TstNode>>,
}

impl Tst {
    /// Build a TST from a slice of keywords.
    /// Empty strings in the keyword list are silently skipped.
    pub fn build(keywords: &[&str]) -> Self {
        let mut tst = Tst { root: None };
        for kw in keywords {
            tst.insert(kw);
        }
        tst
    }

    fn insert(&mut self, key: &str) {
        if key.is_empty() {
            return;
        }
        let chars: Vec<char> = key.chars().collect();
        self.root = Some(Self::insert_node(self.root.take(), &chars, 0));
    }

    fn insert_node(node: Option<Box<TstNode>>, chars: &[char], i: usize) -> Box<TstNode> {
        let ch = chars[i];
        let mut n = node.unwrap_or_else(|| {
            Box::new(TstNode {
                ch,
                is_end: false,
                lo: None,
                eq: None,
                hi: None,
            })
        });
        if ch < n.ch {
            n.lo = Some(Self::insert_node(n.lo.take(), chars, i));
        } else if ch > n.ch {
            n.hi = Some(Self::insert_node(n.hi.take(), chars, i));
        } else if i + 1 < chars.len() {
            n.eq = Some(Self::insert_node(n.eq.take(), chars, i + 1));
        } else {
            n.is_end = true;
        }
        n
    }

    pub fn search(&self, key: &str) -> TstResult {
        if key.is_empty() {
            return TstResult::Prefix;
        }
        let chars: Vec<char> = key.chars().collect();
        Self::search_node(self.root.as_deref(), &chars, 0)
    }

    fn search_node(node: Option<&TstNode>, chars: &[char], i: usize) -> TstResult {
        let n = match node {
            None => return TstResult::Miss,
            Some(n) => n,
        };
        let ch = chars[i];
        if ch < n.ch {
            return Self::search_node(n.lo.as_deref(), chars, i);
        }
        if ch > n.ch {
            return Self::search_node(n.hi.as_deref(), chars, i);
        }
        // ch == n.ch: character matched
        if i + 1 == chars.len() {
            // consumed all characters
            if n.is_end {
                TstResult::Exact
            } else {
                // query is a proper prefix of at least one stored keyword
                TstResult::Prefix
            }
        } else {
            Self::search_node(n.eq.as_deref(), chars, i + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tst() -> Tst {
        Tst::build(&[
            "define", "mutate", "delete", "query", "gate", "form", "gateway",
        ])
    }

    #[test]
    fn test_exact_match() {
        let tst = make_tst();
        assert_eq!(tst.search("define"), TstResult::Exact);
        assert_eq!(tst.search("gateway"), TstResult::Exact);
        assert_eq!(tst.search("gate"), TstResult::Exact); // exact even though "gateway" also exists
    }

    #[test]
    fn test_prefix_match() {
        let tst = make_tst();
        // "def" is a prefix of "define"
        assert_eq!(tst.search("def"), TstResult::Prefix);
        assert_eq!(tst.search("mu"), TstResult::Prefix);
    }

    #[test]
    fn test_miss() {
        let tst = make_tst();
        assert_eq!(tst.search("xyz"), TstResult::Miss);
        assert_eq!(tst.search("definex"), TstResult::Miss);
    }

    #[test]
    fn test_empty_query() {
        let tst = make_tst();
        assert_eq!(tst.search(""), TstResult::Prefix); // empty = valid prefix of everything
    }

    #[test]
    fn test_case_sensitive() {
        let tst = make_tst();
        assert_eq!(tst.search("Define"), TstResult::Miss); // .bit keywords are lowercase
    }

    #[test]
    fn test_exact_with_longer_keyword_present() {
        let tst = Tst::build(&["gateway"]);
        assert_eq!(tst.search("gate"), TstResult::Prefix); // "gate" is prefix of "gateway"
        assert_eq!(tst.search("gateway"), TstResult::Exact);
        assert_eq!(tst.search("gateways"), TstResult::Miss);
    }

    #[test]
    fn test_empty_keyword_skipped_in_build() {
        let tst = Tst::build(&["", "define"]);
        assert_eq!(tst.search("define"), TstResult::Exact);
        assert_eq!(tst.search(""), TstResult::Prefix); // always Prefix regardless
    }
}
