// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.

// Our shared shader includes expect certain bind groups to be in certain spots.
// Note that these are not unique because we need to stay under 4 and thus re-use heavily.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Group {
    Globals,
    Atmosphere,
    Stars,
    TerrainDisplaceMesh,
    TerrainDisplaceTileSet,
    TerrainAccumulateCommon,
    TerrainAccumulateTileSet,
    TerrainComposite,
    Ui,
    OffScreenUi,
    OffScreenWorld,
}

impl Group {
    pub fn index(self) -> u32 {
        match self {
            Self::Atmosphere => 1,
            Self::Globals => 0,
            Self::Stars => 2,
            Self::TerrainDisplaceMesh => 0,
            Self::TerrainDisplaceTileSet => 1,
            Self::TerrainAccumulateCommon => 1,
            Self::TerrainAccumulateTileSet => 2,
            Self::TerrainComposite => 3,
            Self::Ui => 1,
            Self::OffScreenUi => 1,
            Self::OffScreenWorld => 2,
        }
    }
}
