use serde::{Deserialize, Serialize};

pub type PlayerId = usize;
pub type TileId = usize;
pub type VertexId = usize;
pub type EdgeId = usize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CubeCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl CubeCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        debug_assert!(x + y + z == 0);
        Self { x, y, z }
    }

    pub fn neighbors(&self) -> [Self; 6] {
        [
            Self::new(self.x + 1, self.y - 1, self.z),
            Self::new(self.x + 1, self.y, self.z - 1),
            Self::new(self.x, self.y + 1, self.z - 1),
            Self::new(self.x - 1, self.y + 1, self.z),
            Self::new(self.x - 1, self.y, self.z + 1),
            Self::new(self.x, self.y - 1, self.z + 1),
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Brick,
    Lumber,
    Ore,
    Grain,
    Wool,
}

impl Resource {
    pub const fn all() -> [Resource; 5] {
        [
            Resource::Brick,
            Resource::Lumber,
            Resource::Ore,
            Resource::Grain,
            Resource::Wool,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Hills,
    Forest,
    Mountains,
    Fields,
    Pasture,
    Desert,
}

impl Terrain {
    pub fn resource(&self) -> Option<Resource> {
        match self {
            Terrain::Hills => Some(Resource::Brick),
            Terrain::Forest => Some(Resource::Lumber),
            Terrain::Mountains => Some(Resource::Ore),
            Terrain::Fields => Some(Resource::Grain),
            Terrain::Pasture => Some(Resource::Wool),
            Terrain::Desert => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Terrain::Hills => "hills",
            Terrain::Forest => "forest",
            Terrain::Mountains => "mountains",
            Terrain::Fields => "fields",
            Terrain::Pasture => "pasture",
            Terrain::Desert => "desert",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DevelopmentCard {
    Knight,
    VictoryPoint,
    RoadBuilding,
    Invention,
    Monopoly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Port {
    Generic3To1,
    Specific2To1(Resource),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerColor {
    Red,
    Blue,
    White,
    Orange,
}

impl PlayerColor {
    pub fn all() -> [PlayerColor; 4] {
        [
            PlayerColor::Red,
            PlayerColor::Blue,
            PlayerColor::White,
            PlayerColor::Orange,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PlayerColor::Red => "red",
            PlayerColor::Blue => "blue",
            PlayerColor::White => "white",
            PlayerColor::Orange => "orange",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resources {
    pub brick: u8,
    pub lumber: u8,
    pub ore: u8,
    pub grain: u8,
    pub wool: u8,
}

impl Resources {
    /// Create a resource bundle with zero of every resource.
    pub const fn zero() -> Self {
        Self {
            brick: 0,
            lumber: 0,
            ore: 0,
            grain: 0,
            wool: 0,
        }
    }

    /// Get the count for a single resource type.
    pub fn get(&self, resource: Resource) -> u8 {
        match resource {
            Resource::Brick => self.brick,
            Resource::Lumber => self.lumber,
            Resource::Ore => self.ore,
            Resource::Grain => self.grain,
            Resource::Wool => self.wool,
        }
    }

    /// Set the count for a single resource type.
    pub fn set(&mut self, resource: Resource, value: u8) {
        match resource {
            Resource::Brick => self.brick = value,
            Resource::Lumber => self.lumber = value,
            Resource::Ore => self.ore = value,
            Resource::Grain => self.grain = value,
            Resource::Wool => self.wool = value,
        }
    }

    /// Add to a resource count, capping at `u8::MAX`.
    pub fn add(&mut self, resource: Resource, amount: u8) {
        self.set(resource, self.get(resource).saturating_add(amount));
    }

    /// Subtract from a resource count, saturating at zero.
    pub fn sub(&mut self, resource: Resource, amount: u8) {
        self.set(resource, self.get(resource).saturating_sub(amount));
    }

    /// Check whether this bundle covers the cost in every resource.
    pub fn can_afford(&self, cost: &Resources) -> bool {
        self.brick >= cost.brick
            && self.lumber >= cost.lumber
            && self.ore >= cost.ore
            && self.grain >= cost.grain
            && self.wool >= cost.wool
    }

    /// Sum all resource counts.
    pub fn total(&self) -> u8 {
        self.brick + self.lumber + self.ore + self.grain + self.wool
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Building {
    Settlement(PlayerId),
    City(PlayerId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeOffer {
    pub from: PlayerId,
    pub to: PlayerId,
    pub give: Resources,
    pub receive: Resources,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_all_includes_every_resource() {
        let all = Resource::all();
        assert!(all.contains(&Resource::Brick));
        assert!(all.contains(&Resource::Lumber));
        assert!(all.contains(&Resource::Ore));
        assert!(all.contains(&Resource::Grain));
        assert!(all.contains(&Resource::Wool));
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn terrain_resource_mapping() {
        assert_eq!(Terrain::Hills.resource(), Some(Resource::Brick));
        assert_eq!(Terrain::Forest.resource(), Some(Resource::Lumber));
        assert_eq!(Terrain::Mountains.resource(), Some(Resource::Ore));
        assert_eq!(Terrain::Fields.resource(), Some(Resource::Grain));
        assert_eq!(Terrain::Pasture.resource(), Some(Resource::Wool));
        assert_eq!(Terrain::Desert.resource(), None);
    }

    #[test]
    fn terrain_names_are_stable() {
        assert_eq!(Terrain::Hills.name(), "hills");
        assert_eq!(Terrain::Forest.name(), "forest");
        assert_eq!(Terrain::Mountains.name(), "mountains");
        assert_eq!(Terrain::Fields.name(), "fields");
        assert_eq!(Terrain::Pasture.name(), "pasture");
        assert_eq!(Terrain::Desert.name(), "desert");
    }

    #[test]
    fn player_color_all_and_names() {
        let colors = PlayerColor::all();
        assert_eq!(colors.len(), 4);
        assert!(colors.contains(&PlayerColor::Red));
        assert_eq!(PlayerColor::Blue.name(), "blue");
        assert_eq!(PlayerColor::White.name(), "white");
        assert_eq!(PlayerColor::Orange.name(), "orange");
    }

    #[test]
    fn resources_zero_and_get_set() {
        let mut r = Resources::zero();
        assert_eq!(r.get(Resource::Brick), 0);
        r.set(Resource::Lumber, 5);
        assert_eq!(r.get(Resource::Lumber), 5);
    }

    #[test]
    fn resources_arithmetic_saturates_at_zero() {
        let mut r = Resources::zero();
        r.add(Resource::Ore, 3);
        assert_eq!(r.get(Resource::Ore), 3);
        r.sub(Resource::Ore, 10);
        assert_eq!(r.get(Resource::Ore), 0);
    }

    #[test]
    fn resources_can_afford_and_total() {
        let mut r = Resources::zero();
        r.brick = 1;
        r.lumber = 1;
        r.grain = 1;
        r.wool = 1;
        let cost = Resources {
            brick: 1,
            lumber: 1,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        assert!(r.can_afford(&cost));
        r.brick = 0;
        assert!(!r.can_afford(&cost));
        assert_eq!(r.total(), 3);
    }

    #[test]
    fn cube_coord_neighbors_sum_to_zero() {
        let center = CubeCoord::new(0, 0, 0);
        for n in center.neighbors() {
            assert_eq!(n.x + n.y + n.z, 0);
            assert_ne!(n, center);
        }
    }

    #[test]
    fn point_defaults_to_origin() {
        let p = Point::default();
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 0.0);
    }
}
