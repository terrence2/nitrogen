// This file is part of packed_struct.
//
// packed_struct is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// packed_struct is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with packed_struct.  If not, see <http://www.gnu.org/licenses/>.

// Export injector from the wrapper crate
pub use packed_injector::packed_struct;

// Re-export injector deps
pub use anyhow;
pub use zerocopy;
