//! Nodes, ways and relations

use block::str_from_stringtable;
use dense::DenseNode;
use error::Result;
use proto::osmformat;
use proto::osmformat::PrimitiveBlock;
use std;

/// An enum with the OSM core elements: nodes, ways and relations.
#[derive(Clone, Debug)]
pub enum Element<'a> {
    /// A node. Also, see `DenseNode`.
    Node(Node<'a>),

    /// Just like `Node`, but with a different representation in memory. This distinction is
    /// usually not important but is not abstracted away to avoid copying. So, if you want to match
    /// `Node`, you also likely want to match `DenseNode`.
    DenseNode(DenseNode<'a>),

    /// A way.
    Way(Way<'a>),

    /// A relation.
    Relation(Relation<'a>),
}

/// An OpenStreetMap node element (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Node)).
#[derive(Clone, Debug)]
pub struct Node<'a> {
    block: &'a PrimitiveBlock,
    osmnode: &'a osmformat::Node,
}

impl<'a> Node<'a> {
    pub(crate) fn new(block: &'a PrimitiveBlock, osmnode: &'a osmformat::Node) -> Node<'a> {
        Node { block, osmnode }
    }

    /// Returns the node id. It should be unique between nodes and might be negative to indicate
    /// that the element has not yet been uploaded to a server.
    pub fn id(&self) -> i64 {
        self.osmnode.get_id()
    }

    /// Returns an iterator over the tags of this node
    /// (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Tags)).
    /// A tag is represented as a pair of strings (key and value).
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let reader = ElementReader::from_path("tests/test.osm.pbf")?;
    ///
    /// reader.for_each(|element| {
    ///     if let Element::Node(node) = element {
    ///         for (key, value) in node.tags() {
    ///             println!("key: {}, value: {}", key, value);
    ///         }
    ///     }
    /// })?;
    ///
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn tags(&self) -> TagIter<'a> {
        TagIter {
            block: self.block,
            key_indices: self.osmnode.get_keys().iter(),
            val_indices: self.osmnode.get_vals().iter(),
        }
    }

    /// Returns additional metadata for this element.
    pub fn info(&self) -> Info<'a> {
        Info::new(self.block, self.osmnode.get_info())
    }

    /// Returns the latitude coordinate in degrees.
    pub fn lat(&self) -> f64 {
        0.000_000_001_f64 * self.lat_in_nano_degrees() as f64
    }

    /// Returns the longitude coordinate in degrees.
    pub fn lon(&self) -> f64 {
        0.000_000_001_f64 * self.lon_in_nano_degrees() as f64
    }

    /// Returns the latitude coordinate in nano-degrees.
    pub fn lat_in_nano_degrees(&self) -> i64 {
        self.block.get_lat_offset()
            + (i64::from(self.block.get_granularity()) * self.osmnode.get_lat())
    }

    /// Returns the longitude coordinate in nano-degrees.
    pub fn lon_in_nano_degrees(&self) -> i64 {
        self.block.get_lon_offset()
            + (i64::from(self.block.get_granularity()) * self.osmnode.get_lon())
    }

    /// Returns an iterator over the tags of this node
    /// (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Tags)).
    /// A tag is represented as a pair of indices (key and value) to the stringtable of the current
    /// `PrimitiveBlock`.
    pub fn raw_tags(&self) -> RawTagIter<'a> {
        RawTagIter {
            block: self.block,
            key_indices: self.osmnode.get_keys().iter(),
            val_indices: self.osmnode.get_vals().iter(),
        }
    }

    /// Returns the raw stringtable. Elements in a `PrimitiveBlock` do not store strings
    /// themselves; instead, they just store indices to a common stringtable. By convention, the
    /// contained strings are UTF-8 encoded but it is not safe to assume that (use
    /// `std::str::from_utf8`).
    pub fn raw_stringtable(&self) -> &[Vec<u8>] {
        self.block.get_stringtable().get_s()
    }
}

/// An OpenStreetMap way element (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Way)).
///
/// A way contains an ordered list of node references that can be accessed with the `refs` or the
/// `raw_refs` method.
#[derive(Clone, Debug)]
pub struct Way<'a> {
    block: &'a PrimitiveBlock,
    osmway: &'a osmformat::Way,
}

impl<'a> Way<'a> {
    pub(crate) fn new(block: &'a PrimitiveBlock, osmway: &'a osmformat::Way) -> Way<'a> {
        Way { block, osmway }
    }

    /// Returns the way id.
    pub fn id(&self) -> i64 {
        self.osmway.get_id()
    }

    /// Returns an iterator over the tags of this way
    /// (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Tags)).
    /// A tag is represented as a pair of strings (key and value).
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let reader = ElementReader::from_path("tests/test.osm.pbf")?;
    ///
    /// reader.for_each(|element| {
    ///     if let Element::Way(way) = element {
    ///         for (key, value) in way.tags() {
    ///             println!("key: {}, value: {}", key, value);
    ///         }
    ///     }
    /// })?;
    ///
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn tags(&self) -> TagIter<'a> {
        TagIter {
            block: self.block,
            key_indices: self.osmway.get_keys().iter(),
            val_indices: self.osmway.get_vals().iter(),
        }
    }

    /// Returns additional metadata for this element.
    pub fn info(&self) -> Info<'a> {
        Info::new(self.block, self.osmway.get_info())
    }

    /// Returns an iterator over the references of this way. Each reference should correspond to a
    /// node id.
    pub fn refs(&self) -> WayRefIter<'a> {
        WayRefIter {
            deltas: self.osmway.get_refs().iter(),
            current: 0,
        }
    }

    /// Returns a slice of delta coded node ids.
    pub fn raw_refs(&self) -> &[i64] {
        self.osmway.get_refs()
    }

    /// Returns an iterator over the tags of this way
    /// (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Tags)).
    /// A tag is represented as a pair of indices (key and value) to the stringtable of the current
    /// `PrimitiveBlock`.
    pub fn raw_tags(&self) -> RawTagIter<'a> {
        RawTagIter {
            block: self.block,
            key_indices: self.osmway.get_keys().iter(),
            val_indices: self.osmway.get_vals().iter(),
        }
    }

    /// Returns the raw stringtable. Elements in a `PrimitiveBlock` do not store strings
    /// themselves; instead, they just store indices to a common stringtable. By convention, the
    /// contained strings are UTF-8 encoded but it is not safe to assume that (use
    /// `std::str::from_utf8`).
    pub fn raw_stringtable(&self) -> &[Vec<u8>] {
        self.block.get_stringtable().get_s()
    }
}

/// An OpenStreetMap relation element (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Relation)).
///
/// A relation contains an ordered list of members that can be of any element type.
#[derive(Clone, Debug)]
pub struct Relation<'a> {
    block: &'a PrimitiveBlock,
    osmrel: &'a osmformat::Relation,
}

impl<'a> Relation<'a> {
    pub(crate) fn new(block: &'a PrimitiveBlock, osmrel: &'a osmformat::Relation) -> Relation<'a> {
        Relation { block, osmrel }
    }

    /// Returns the relation id.
    pub fn id(&self) -> i64 {
        self.osmrel.get_id()
    }

    /// Returns an iterator over the tags of this relation
    /// (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Tags)).
    /// A tag is represented as a pair of strings (key and value).
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let reader = ElementReader::from_path("tests/test.osm.pbf")?;
    ///
    /// reader.for_each(|element| {
    ///     if let Element::Relation(relation) = element {
    ///         for (key, value) in relation.tags() {
    ///             println!("key: {}, value: {}", key, value);
    ///         }
    ///     }
    /// })?;
    ///
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn tags(&self) -> TagIter<'a> {
        TagIter {
            block: self.block,
            key_indices: self.osmrel.get_keys().iter(),
            val_indices: self.osmrel.get_vals().iter(),
        }
    }

    /// Returns additional metadata for this element.
    pub fn info(&self) -> Info<'a> {
        Info::new(self.block, self.osmrel.get_info())
    }

    /// Returns an iterator over the members of this relation.
    pub fn members(&self) -> RelMemberIter<'a> {
        RelMemberIter::new(self.block, self.osmrel)
    }

    /// Returns an iterator over the tags of this relation
    /// (See [OSM wiki](http://wiki.openstreetmap.org/wiki/Tags)).
    /// A tag is represented as a pair of indices (key and value) to the stringtable of the current
    /// `PrimitiveBlock`.
    pub fn raw_tags(&self) -> RawTagIter<'a> {
        RawTagIter {
            block: self.block,
            key_indices: self.osmrel.get_keys().iter(),
            val_indices: self.osmrel.get_vals().iter(),
        }
    }

    /// Returns the raw stringtable. Elements in a `PrimitiveBlock` do not store strings
    /// themselves; instead, they just store indices to a common stringtable. By convention, the
    /// contained strings are UTF-8 encoded but it is not safe to assume that (use
    /// `std::str::from_utf8`).
    pub fn raw_stringtable(&self) -> &[Vec<u8>] {
        self.block.get_stringtable().get_s()
    }
}

/// An iterator over the references of a way.
///
/// Each reference corresponds to a node id.
#[derive(Clone, Debug)]
pub struct WayRefIter<'a> {
    deltas: std::slice::Iter<'a, i64>,
    current: i64,
}

impl<'a> Iterator for WayRefIter<'a> {
    type Item = i64;

    fn next(&mut self) -> Option<Self::Item> {
        match self.deltas.next() {
            Some(&d) => {
                self.current += d;
                Some(self.current)
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.deltas.size_hint()
    }
}

impl<'a> ExactSizeIterator for WayRefIter<'a> {}

/// The element type of a relation member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelMemberType {
    Node,
    Way,
    Relation,
}

impl From<osmformat::Relation_MemberType> for RelMemberType {
    fn from(rmt: osmformat::Relation_MemberType) -> RelMemberType {
        match rmt {
            osmformat::Relation_MemberType::NODE => RelMemberType::Node,
            osmformat::Relation_MemberType::WAY => RelMemberType::Way,
            osmformat::Relation_MemberType::RELATION => RelMemberType::Relation,
        }
    }
}

//TODO encapsulate member_id based on member_type (NodeId, WayId, RelationId)
/// A member of a relation.
///
/// Each member has a member type and a member id that references an element of that type.
#[derive(Clone, Debug)]
pub struct RelMember<'a> {
    block: &'a PrimitiveBlock,
    pub role_sid: i32,
    pub member_id: i64,
    pub member_type: RelMemberType,
}

impl<'a> RelMember<'a> {
    /// Returns the role of a relation member.
    pub fn role(&self) -> Result<&'a str> {
        str_from_stringtable(self.block, self.role_sid as usize)
    }
}

/// An iterator over the members of a relation.
#[derive(Clone, Debug)]
pub struct RelMemberIter<'a> {
    block: &'a PrimitiveBlock,
    role_sids: std::slice::Iter<'a, i32>,
    member_id_deltas: std::slice::Iter<'a, i64>,
    member_types: std::slice::Iter<'a, osmformat::Relation_MemberType>,
    current_member_id: i64,
}

impl<'a> RelMemberIter<'a> {
    fn new(block: &'a PrimitiveBlock, osmrel: &'a osmformat::Relation) -> RelMemberIter<'a> {
        RelMemberIter {
            block,
            role_sids: osmrel.get_roles_sid().iter(),
            member_id_deltas: osmrel.get_memids().iter(),
            member_types: osmrel.get_types().iter(),
            current_member_id: 0,
        }
    }
}

impl<'a> Iterator for RelMemberIter<'a> {
    type Item = RelMember<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match (
            self.role_sids.next(),
            self.member_id_deltas.next(),
            self.member_types.next(),
        ) {
            (Some(role_sid), Some(mem_id_delta), Some(member_type)) => {
                self.current_member_id += *mem_id_delta;
                Some(RelMember {
                    block: self.block,
                    role_sid: *role_sid,
                    member_id: self.current_member_id,
                    member_type: RelMemberType::from(*member_type),
                })
            }
            _ => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.role_sids.size_hint()
    }
}

impl<'a> ExactSizeIterator for RelMemberIter<'a> {}

/// An iterator over the tags of an element. It returns a pair of strings (key and value).
#[derive(Clone, Debug)]
pub struct TagIter<'a> {
    block: &'a PrimitiveBlock,
    key_indices: std::slice::Iter<'a, u32>,
    val_indices: std::slice::Iter<'a, u32>,
}

//TODO return Result?
impl<'a> Iterator for TagIter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.key_indices.next(), self.val_indices.next()) {
            (Some(&key_index), Some(&val_index)) => {
                let k_res = str_from_stringtable(self.block, key_index as usize);
                let v_res = str_from_stringtable(self.block, val_index as usize);
                if let (Ok(k), Ok(v)) = (k_res, v_res) {
                    Some((k, v))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.key_indices.size_hint()
    }
}

impl<'a> ExactSizeIterator for TagIter<'a> {}

/// An iterator over the tags of an element. It returns a pair of indices (key and value) to the
/// stringtable of the current `PrimitiveBlock`.
#[derive(Clone, Debug)]
pub struct RawTagIter<'a> {
    block: &'a PrimitiveBlock,
    key_indices: std::slice::Iter<'a, u32>,
    val_indices: std::slice::Iter<'a, u32>,
}

//TODO return Result?
impl<'a> Iterator for RawTagIter<'a> {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.key_indices.next(), self.val_indices.next()) {
            (Some(&key_index), Some(&val_index)) => Some((key_index, val_index)),
            _ => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.key_indices.size_hint()
    }
}

impl<'a> ExactSizeIterator for RawTagIter<'a> {}

/// Additional metadata that might be included in each element.
#[derive(Clone, Debug)]
pub struct Info<'a> {
    block: &'a PrimitiveBlock,
    info: &'a osmformat::Info,
}

impl<'a> Info<'a> {
    fn new(block: &'a PrimitiveBlock, info: &'a osmformat::Info) -> Info<'a> {
        Info { block, info }
    }

    /// Returns the version of this element.
    pub fn version(&self) -> Option<i32> {
        if self.info.has_version() {
            Some(self.info.get_version())
        } else {
            None
        }
    }

    /// Returns the time stamp in milliseconds since the epoch.
    pub fn milli_timestamp(&self) -> Option<i64> {
        if self.info.has_timestamp() {
            Some(self.info.get_timestamp() * i64::from(self.block.get_date_granularity()))
        } else {
            None
        }
    }

    /// Returns the changeset id.
    pub fn changeset(&self) -> Option<i64> {
        if self.info.has_changeset() {
            Some(self.info.get_changeset())
        } else {
            None
        }
    }

    /// Returns the user id.
    pub fn uid(&self) -> Option<i32> {
        if self.info.has_uid() {
            Some(self.info.get_uid())
        } else {
            None
        }
    }

    /// Returns the user name.
    pub fn user(&self) -> Option<Result<&'a str>> {
        if self.info.has_user_sid() {
            Some(str_from_stringtable(
                self.block,
                self.info.get_user_sid() as usize,
            ))
        } else {
            None
        }
    }

    /// Returns the visibility status of an element. This is only relevant if the PBF file contains
    /// historical information.
    pub fn visible(&self) -> bool {
        if self.info.has_visible() {
            self.info.get_visible()
        } else {
            // If the visible flag is not present it must be assumed to be true.
            true
        }
    }
}
