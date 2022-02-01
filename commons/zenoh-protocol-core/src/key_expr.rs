//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

use crate::ExprId;
use core::fmt;
use std::{borrow::Cow, convert::TryInto};
use zenoh_core::{bail, Result as ZResult};

#[inline(always)]
fn cend(s: &str) -> bool {
    s.is_empty() || s.starts_with('/')
}

#[inline(always)]
fn cwild(s: &str) -> bool {
    s.starts_with('*')
}

#[inline(always)]
fn cnext(s: &str) -> &str {
    &s[1..]
}

#[inline(always)]
fn cequal(s1: &str, s2: &str) -> bool {
    s1.starts_with(&s2[0..1])
}

macro_rules! DEFINE_INTERSECT {
    ($name:ident, $end:ident, $wild:ident, $next:ident, $elem_intersect:ident) => {
        fn $name(c1: &str, c2: &str) -> bool {
            if ($end(c1) && $end(c2)) {
                return true;
            }
            if ($wild(c1) && $end(c2)) {
                return $name($next(c1), c2);
            }
            if ($end(c1) && $wild(c2)) {
                return $name(c1, $next(c2));
            }
            if ($wild(c1)) {
                if ($end($next(c1))) {
                    return true;
                }
                if ($name($next(c1), c2)) {
                    return true;
                } else {
                    return $name(c1, $next(c2));
                }
            }
            if ($wild(c2)) {
                if ($end($next(c2))) {
                    return true;
                }
                if ($name($next(c1), c2)) {
                    return true;
                } else {
                    return $name(c1, $next(c2));
                }
            }
            if ($end(c1) || $end(c2)) {
                return false;
            }
            if ($elem_intersect(c1, c2)) {
                return $name($next(c1), $next(c2));
            }
            return false;
        }
    };
}

macro_rules! DEFINE_INCLUDE {
    ($name:ident, $end:ident, $wild:ident, $next:ident, $elem_include:ident) => {
        fn $name(this: &str, sub: &str) -> bool {
            if ($end(this) && $end(sub)) {
                return true;
            }
            if ($wild(this) && $end(sub)) {
                return $name($next(this), sub);
            }
            if ($wild(this)) {
                if ($end($next(this))) {
                    return true;
                }
                if ($name($next(this), sub)) {
                    return true;
                } else {
                    return $name(this, $next(sub));
                }
            }
            if ($wild(sub)) {
                return false;
            }
            if ($end(this) || $end(sub)) {
                return false;
            }
            if ($elem_include(this, sub)) {
                return $name($next(this), $next(sub));
            }
            return false;
        }
    };
}

DEFINE_INTERSECT!(sub_chunk_intersect, cend, cwild, cnext, cequal);

#[inline(always)]
fn chunk_intersect(c1: &str, c2: &str) -> bool {
    if (cend(c1) && !cend(c2)) || (!cend(c1) && cend(c2)) {
        return false;
    }
    sub_chunk_intersect(c1, c2)
}

DEFINE_INCLUDE!(chunk_include, cend, cwild, cnext, cequal);

#[inline(always)]
fn end(s: &str) -> bool {
    s.is_empty()
}

#[inline(always)]
fn wild(s: &str) -> bool {
    s.starts_with("**/") || s == "**"
}

#[inline(always)]
fn next(s: &str) -> &str {
    match s.find('/') {
        Some(idx) => &s[(idx + 1)..],
        None => "",
    }
}

DEFINE_INTERSECT!(res_intersect, end, wild, next, chunk_intersect);

/// Retruns `true` if the given key expressions intersect.
///
/// I.e. if it exists a resource key (with no wildcards) that matches
/// both given key expressions.
#[inline(always)]
pub fn intersect(s1: &str, s2: &str) -> bool {
    res_intersect(s1, s2)
}

DEFINE_INCLUDE!(res_include, end, wild, next, chunk_include);

/// Retruns `true` if the first key expression (`this`) includes the second key expression (`sub`).
///
/// I.e. if there exists no resource key (with no wildcards) that matches
/// `sub` but does not match `this`.
#[inline(always)]
pub fn include(this: &str, sub: &str) -> bool {
    res_include(this, sub)
}

pub const ADMIN_PREFIX: &str = "/@/";

#[inline(always)]
pub fn matches(s1: &str, s2: &str) -> bool {
    if s1.starts_with(ADMIN_PREFIX) == s2.starts_with(ADMIN_PREFIX) {
        intersect(s1, s2)
    } else {
        false
    }
}

/// A zenoh **resource** is represented by a pair composed by a **key** and a
/// **value**, such as, ```(/car/telemetry/speed, 320)```.  A **resource key**
/// is an arbitrary array of characters, with the exclusion of the symbols
/// ```*```, ```**```, ```?```, ```[```, ```]```, and ```#```,
/// which have special meaning in the context of zenoh.
///
/// A key including any number of the wildcard symbols, ```*``` and ```**```,
/// such as, ```/car/telemetry/*```, is called a **key expression** as it
/// denotes a set of keys. The wildcard character ```*``` expands to an
/// arbitrary string not including zenoh's reserved characters and the ```/```
/// character, while the ```**``` expands to  strings that may also include the
/// ```/``` character.  
///
/// Finally, it is worth mentioning that for time and space efficiency matters,
/// zenoh will automatically map key expressions to small integers. The mapping is automatic,
/// but it can be triggered excplicily by with [`declare_expr`](crate::Session::declare_expr).
///
//
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~      id       — if Expr : id=0
// +-+-+-+-+-+-+-+-+
// ~    suffix     ~ if flag K==1 in Message's header
// +---------------+
//
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct KeyExpr<'a> {
    pub scope: ExprId, // 0 marks global scope
    pub suffix: Cow<'a, str>,
}

impl<'a> KeyExpr<'a> {
    pub fn as_str(&'a self) -> &'a str {
        if self.scope == 0 {
            self.suffix.as_ref()
        } else {
            "<encoded_expr>"
        }
    }

    pub fn try_as_str(&'a self) -> ZResult<&'a str> {
        if self.scope == 0 {
            Ok(self.suffix.as_ref())
        } else {
            bail!("Scoped key expression")
        }
    }

    pub fn as_id(&'a self) -> ExprId {
        self.scope
    }

    pub fn try_as_id(&'a self) -> ZResult<ExprId> {
        if self.has_suffix() {
            bail!("Suffixed key expression")
        } else {
            Ok(self.scope)
        }
    }

    pub fn as_id_and_suffix(&'a self) -> (ExprId, &'a str) {
        (self.scope, self.suffix.as_ref())
    }

    pub fn has_suffix(&self) -> bool {
        !self.suffix.as_ref().is_empty()
    }

    pub fn to_owned(&self) -> KeyExpr<'static> {
        KeyExpr {
            scope: self.scope,
            suffix: self.suffix.to_string().into(),
        }
    }

    pub fn with_suffix(mut self, suffix: &'a str) -> Self {
        if self.suffix.is_empty() {
            self.suffix = suffix.into();
        } else {
            self.suffix += suffix;
        }
        self
    }
}

impl TryInto<String> for KeyExpr<'_> {
    type Error = zenoh_core::Error;
    fn try_into(self) -> Result<String, Self::Error> {
        if self.scope == 0 {
            Ok(self.suffix.into_owned())
        } else {
            bail!("Scoped key expression")
        }
    }
}

impl TryInto<ExprId> for KeyExpr<'_> {
    type Error = zenoh_core::Error;
    fn try_into(self) -> Result<ExprId, Self::Error> {
        self.try_as_id()
    }
}

impl fmt::Debug for KeyExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.scope == 0 {
            write!(f, "{}", self.suffix)
        } else {
            write!(f, "{}:{}", self.scope, self.suffix)
        }
    }
}

impl fmt::Display for KeyExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.scope == 0 {
            write!(f, "{}", self.suffix)
        } else {
            write!(f, "{}:{}", self.scope, self.suffix)
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for KeyExpr<'a> {
    #[inline]
    fn from(key: &KeyExpr<'a>) -> KeyExpr<'a> {
        key.clone()
    }
}

impl From<ExprId> for KeyExpr<'_> {
    #[inline]
    fn from(rid: ExprId) -> KeyExpr<'static> {
        KeyExpr {
            scope: rid,
            suffix: "".into(),
        }
    }
}

impl From<&ExprId> for KeyExpr<'_> {
    #[inline]
    fn from(rid: &ExprId) -> KeyExpr<'static> {
        KeyExpr {
            scope: *rid,
            suffix: "".into(),
        }
    }
}

impl<'a> From<&'a str> for KeyExpr<'a> {
    #[inline]
    fn from(name: &'a str) -> KeyExpr<'a> {
        KeyExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

impl From<String> for KeyExpr<'_> {
    #[inline]
    fn from(name: String) -> KeyExpr<'static> {
        KeyExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

impl<'a> From<&'a String> for KeyExpr<'a> {
    #[inline]
    fn from(name: &'a String) -> KeyExpr<'a> {
        KeyExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}