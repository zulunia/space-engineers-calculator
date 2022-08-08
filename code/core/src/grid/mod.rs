use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use direction::PerDirection;

use crate::data::blocks::{BlockId, ThrusterType};
use crate::data::Data;
use crate::grid::direction::{CountPerDirection, Direction};

pub mod direction;

// Battery mode

#[derive(Default, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize, Debug)]
pub enum BatteryMode {
  Auto,
  Recharge,
  #[default] Discharge,
  Off,
}

impl BatteryMode {
  #[inline]
  pub fn items() -> impl IntoIterator<Item=Self> {
    use BatteryMode::*;
    const ITEMS: [BatteryMode; 4] = [Auto, Recharge, Discharge, Off];
    ITEMS.into_iter()
  }

  #[inline]
  pub fn is_charging(&self) -> bool {
    use BatteryMode::*;
    match self { Auto => true, Recharge => true, _ => false }
  }

  #[inline]
  pub fn is_discharging(&self) -> bool {
    use BatteryMode::*;
    match self { Auto => true, Discharge => true, _ => false }
  }
}

impl Display for BatteryMode {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    use BatteryMode::*;
    match self {
      Auto => f.write_str("Auto"),
      Recharge => f.write_str("Recharge"),
      Discharge => f.write_str("Discharge"),
      Off => f.write_str("Off"),
    }
  }
}


// Calculator

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct GridCalculator {
  /// Gravity multiplier 0-* (g)
  pub gravity_multiplier: f64,
  /// Container multiplier 0-*
  pub container_multiplier: f64,
  /// Planetary influence 0-1
  pub planetary_influence: f64,
  /// Battery mode
  pub battery_mode: BatteryMode,
  /// Hydrogen tanks enabled?
  pub hydrogen_tank_enabled: bool,
  /// Hydrogen engines enabled?
  pub hydrogen_engine_enabled: bool,
  /// Are railguns charging?
  pub railgun_charging: bool,
  /// Are jump drives charging?
  pub jump_drive_charging: bool,
  /// Thruster power 0-100%
  pub thruster_power: f64,
  /// Wheel power 0-100%
  pub wheel_power: f64,
  /// Additional mass (kg)
  pub additional_mass: f64,
  /// Ice only fill 0-100%
  pub ice_only_fill: f64,
  /// Ore only fill 0-100%
  pub ore_only_fill: f64,
  /// Any fill with ice 0-100%
  pub any_fill_with_ice: f64,
  /// Any fill with ore 0-100%
  pub any_fill_with_ore: f64,
  /// Any fill with steel plates 0-100%
  pub any_fill_with_steel_plates: f64,
  /// Block counts
  pub blocks: HashMap<BlockId, u64>,
  /// Block counts per direction.
  pub directional_blocks: HashMap<BlockId, CountPerDirection>,
}

impl Default for GridCalculator {
  fn default() -> Self {
    Self {
      gravity_multiplier: 1.0,
      container_multiplier: 1.0,
      planetary_influence: 1.0,
      battery_mode: Default::default(),
      hydrogen_tank_enabled: true,
      hydrogen_engine_enabled: true,
      railgun_charging: true,
      jump_drive_charging: true,
      thruster_power: 100.0,
      wheel_power: 100.0,
      ice_only_fill: 100.0,
      ore_only_fill: 100.0,
      any_fill_with_ice: 0.0,
      any_fill_with_ore: 0.0,
      any_fill_with_steel_plates: 0.0,
      additional_mass: 0.0,
      blocks: Default::default(),
      directional_blocks: Default::default(),
    }
  }
}

const MWH_TO_MINUTES: f64 = 60.0;
const LS_TO_MINUTES: f64 = 1.0 / 60.0;

impl GridCalculator {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn iter_block_counts(&self) -> impl Iterator<Item=(&BlockId, &u64)> {
    self.blocks.iter()
  }

  pub fn calculate(&self, data: &Data) -> GridCalculated {
    let ice_weight_per_volume = 1.0 / 0.37; // TODO: derive from data
    let ice_items_per_volume = 1.0 / 0.37; // TODO: derive from data
    let ore_weight_per_volume = 1.0 / 0.37; // TODO: derive from data
    let ore_items_per_volume = 1.0 / 0.37; // TODO: derive from data
    let steel_plate_weight_per_volume = 20.0 / 3.0; // TODO: derive from data
    let steel_plate_items_per_volume = 1.0 / 3.0; // TODO: derive from data

    let mut c = GridCalculated::default();

    let mut power_consumption_idle = 0.0;
    let mut power_consumption_railgun = 0.0;
    let mut power_consumption_utility = 0.0;
    let mut power_consumption_wheel_suspension = 0.0;
    let mut power_consumption_jump_drive = 0.0;
    let mut power_consumption_generator = 0.0;
    let mut power_consumption_thruster: PerDirection<f64> = PerDirection::default();
    let mut power_consumption_battery = 0.0;

    let mut hydrogen_consumption_idle = 0.0;
    let mut hydrogen_consumption_engine = 0.0;
    let mut hydrogen_consumption_thruster: PerDirection<f64> = PerDirection::default();

    let mut jump_power = 0.0;
    let mut max_jump_distance = 0.0;

    c.total_mass_empty += self.additional_mass;

    // Non-directional blocks
    let wheel_power_ratio = self.wheel_power / 100.0;
    for (id, count) in self.blocks.iter().filter(|(_, c)| **c != 0) {
      let count = *count as f64;
      if let Some(block) = data.blocks.containers.get(id) { // Containers.
        c.total_mass_empty += block.mass(&data.components) * count;
        if block.store_any {
          let volume = block.details.inventory_volume_any * count * self.container_multiplier;
          c.total_volume_any += volume;
          c.total_volume_ore += volume;
          c.total_volume_ice += volume;
        }
      } else if let Some(block) = data.blocks.connectors.get(id) {// Connectors.
        c.total_mass_empty += block.mass(&data.components) * count;
        let volume = block.details.inventory_volume_any * count * self.container_multiplier;
        c.total_volume_any += volume;
        c.total_volume_ore += volume;
        c.total_volume_ice += volume;
      } else if let Some(block) = data.blocks.cockpits.get(id) { // Cockpits.
        c.total_mass_empty += block.mass(&data.components) * count;
        if block.has_inventory {
          let volume = block.details.inventory_volume_any * count * self.container_multiplier;
          c.total_volume_any += volume;
          c.total_volume_ore += volume;
          c.total_volume_ice += volume;
        }
      } else if let Some(block) = data.blocks.wheel_suspensions.get(id) { // Wheel suspensions
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        c.wheel_force += details.force * count * wheel_power_ratio;
        power_consumption_idle += details.idle_power_consumption * count;
        power_consumption_wheel_suspension += details.operational_power_consumption * count * wheel_power_ratio;
      } else if let Some(block) = data.blocks.hydrogen_engines.get(id) { // Hydrogen Engines.
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        if self.hydrogen_engine_enabled {
          c.power_generation += details.max_power_generation * count;
          hydrogen_consumption_engine += details.max_fuel_consumption * count;
          c.hydrogen_engine_enabled = true;
        }
        *c.hydrogen_engine_capacity.get_or_insert(0.0) += details.fuel_capacity * count;
        c.hydrogen_tank_enabled = true;
      } else if let Some(block) = data.blocks.reactors.get(id) {  // Reactors.
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        c.power_generation += details.max_power_generation * count;
        // TODO: inventory - uranium ingot only
        // TODO: fuel capacity/use
      } else if let Some(block) = data.blocks.batteries.get(id) { // Batteries.
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        if self.battery_mode.is_discharging() {
          c.power_generation += details.output * count;
          c.battery_discharging = true;
        }
        if self.battery_mode.is_charging() {
          power_consumption_battery += details.input * count;
        }
        *c.battery_capacity.get_or_insert(0.0) += details.capacity * count;
      } else if let Some(block) = data.blocks.jump_drives.get(id) { // Jump drives
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        *c.jump_drive_capacity.get_or_insert(0.0) += block.capacity * count;
        if self.jump_drive_charging {
          power_consumption_jump_drive += details.operational_power_consumption * count;
        }
        // Formula based on https://www.spaceengineerswiki.com/Jump_drive
        let max_jump_drive_distance = details.max_jump_distance / 1000.0; // Convert from m to km.
        jump_power += max_jump_drive_distance * details.max_jump_mass * count;
        max_jump_distance += max_jump_drive_distance * count;
      } else if let Some(block) = data.blocks.railguns.get(id) { // Railguns
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        *c.railgun_capacity.get_or_insert(0.0) += block.capacity * count;
        power_consumption_idle += details.idle_power_consumption * count;
        if self.railgun_charging {
          power_consumption_railgun += details.operational_power_consumption * count;
        }
      } else if let Some(block) = data.blocks.generators.get(id) { // Hydrogen Generators.
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        c.total_volume_ice_only += details.inventory_volume_ice * count;
        power_consumption_idle += details.idle_power_consumption * count;
        power_consumption_generator += details.operational_power_consumption * count;
        c.hydrogen_generation += details.hydrogen_generation * count;
        // TODO: ice consumption
      } else if let Some(block) = data.blocks.hydrogen_tanks.get(id) { // Hydrogen Tanks.
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        power_consumption_idle += details.idle_power_consumption * count;
        power_consumption_utility += details.operational_power_consumption * count;
        *c.hydrogen_tank_capacity.get_or_insert(0.0) += details.capacity * count;
      } else if let Some(block) = data.blocks.drills.get(id) { // Drills
        let details = &block.details;
        c.total_mass_empty += block.mass(&data.components) * count;
        c.total_volume_ore_only += details.inventory_volume_ore * count;
        power_consumption_idle += details.idle_power_consumption * count;
        power_consumption_utility += details.operational_power_consumption * count;
      }
    }
    // Directional blocks
    let thruster_power_ratio = self.thruster_power / 100.0;
    for (id, count_per_direction) in self.directional_blocks.iter() {
      for (direction, count) in count_per_direction.iter_with_direction() {
        if let Some(block) = data.blocks.thrusters.get(id) { // Thrusters
          let count = *count as f64;
          let details = &block.details;
          c.total_mass_empty += block.mass(&data.components) * count;
          // Clamp planetary influence value.
          let planetary_influence = self.planetary_influence.clamp(details.min_planetary_influence, details.max_planetary_influence);
          // Slope-intercept form equation: y = mx + b
          // Calculate m: m = (y2 - y1) / (x2 - x1)
          let m = (details.effectiveness_at_min_influence - details.effectiveness_at_max_influence) / (details.min_planetary_influence - details.max_planetary_influence);
          // Calculate b: b = y + -mx (choose x,y on the line)
          let b = details.effectiveness_at_max_influence + (-1.0 * m * details.max_planetary_influence);
          // Calculate y: y = mx + b
          let effectiveness = m * planetary_influence + b;
          c.thruster_acceleration[direction].force += details.force * thruster_power_ratio * effectiveness * count;
          match details.ty {
            ThrusterType::Hydrogen => {
              hydrogen_consumption_idle += details.actual_min_consumption(&data.gas_properties) * count;
              let max_consumption = details.actual_max_consumption(&data.gas_properties) * thruster_power_ratio * count; // TODO: should this be multiplied with `effectiveness`?
              hydrogen_consumption_thruster[direction] += max_consumption;
            },
            _ => {
              power_consumption_idle += details.actual_min_consumption(&data.gas_properties) * count;
              let max_consumption = details.actual_max_consumption(&data.gas_properties) * thruster_power_ratio * count; // TODO: should this be multiplied with `effectiveness`?
              power_consumption_thruster[direction] += max_consumption;
            },
          }
        }
      }
    }

    // Calculate filled volumes.
    let ice_only_volume = c.total_volume_ice_only * (self.ice_only_fill / 100.0);
    let ore_only_volume = c.total_volume_ore_only * (self.ore_only_fill / 100.0);
    let ice_in_any_volume = c.total_volume_any * (self.any_fill_with_ice / 100.0);
    let ore_in_any_volume = c.total_volume_any * (self.any_fill_with_ore / 100.0);
    let steel_plates_in_any_volume = c.total_volume_any * (self.any_fill_with_steel_plates / 100.0);

    // Calculate filled mass.
    // TODO: container multiplier increases volume but keeps mass the same!
    let ice_only_mass = ice_only_volume * ice_weight_per_volume;
    let ore_only_mass = ore_only_volume * ore_weight_per_volume;
    let any_mass = (ice_in_any_volume * ice_weight_per_volume) + (ore_in_any_volume * ore_weight_per_volume) + (steel_plates_in_any_volume * steel_plate_weight_per_volume);
    c.total_mass_filled = c.total_mass_empty + ice_only_mass + ore_only_mass + any_mass;

    // Calculate filled items.
    c.total_items_ore = (ore_only_volume + ore_in_any_volume) * ore_items_per_volume;
    c.total_items_ice = (ice_only_volume + ice_in_any_volume) * ice_items_per_volume;
    c.total_items_steel_plate = steel_plates_in_any_volume * steel_plate_items_per_volume;

    // Calculate Acceleration
    let has_mass_empty = c.total_mass_empty != 0.0;
    let has_mass_filled = c.total_mass_filled != 0.0;
    for a in c.thruster_acceleration.iter_mut() {
      a.acceleration_empty_no_gravity = has_mass_empty.then(|| a.force / c.total_mass_empty);
      a.acceleration_filled_no_gravity = has_mass_filled.then(|| a.force / c.total_mass_filled);
      a.acceleration_empty_gravity = has_mass_empty.then(|| (a.force - (c.total_mass_empty * 9.81 * self.gravity_multiplier)) / c.total_mass_empty);
      a.acceleration_filled_gravity = has_mass_filled.then(|| (a.force - (c.total_mass_filled * 9.81 * self.gravity_multiplier)) / c.total_mass_filled);
    }

    let (actual_power_consumption_railgun, actual_power_consumption_jump_drive, actual_power_consumption_battery) = {
      // Idle
      c.power_idle = c.power_resource(power_consumption_idle, power_consumption_idle);

      // Non-idle
      // Defense (railgun)
      let actual_power_consumption_railgun = power_consumption_railgun.min(c.power_generation).max(0.0);
      let mut total_consumption = power_consumption_railgun;
      c.power_railgun = c.power_resource(power_consumption_railgun, total_consumption);
      // Utility
      total_consumption += power_consumption_utility;
      c.power_upto_utility = c.power_resource(power_consumption_utility, total_consumption);
      // Utility (wheel suspensions)
      total_consumption += power_consumption_wheel_suspension;
      c.power_upto_wheel_suspension = c.power_resource(power_consumption_wheel_suspension, total_consumption);
      // Charge jump drive
      let actual_power_consumption_jump_drive = power_consumption_jump_drive.min(c.power_upto_wheel_suspension.balance).max(0.0);
      total_consumption += power_consumption_jump_drive;
      c.power_upto_jump_drive = c.power_resource(power_consumption_jump_drive, total_consumption);
      // Generator
      total_consumption += power_consumption_generator;
      c.power_upto_generator = c.power_resource(power_consumption_generator, total_consumption);
      // Thrust - Up/Down
      let up_down_consumption = Self::thruster_consumption_peak(&power_consumption_thruster, Direction::Up, Direction::Down);
      total_consumption += up_down_consumption;
      c.power_upto_up_down_thruster = c.power_resource(up_down_consumption, total_consumption);
      // Thrust - Front/Back
      let front_back_consumption = Self::thruster_consumption_peak(&power_consumption_thruster, Direction::Front, Direction::Back);
      total_consumption += front_back_consumption;
      c.power_upto_front_back_thruster = c.power_resource(front_back_consumption, total_consumption);
      // Thrust - Left/Right
      let left_right_consumption = Self::thruster_consumption_peak(&power_consumption_thruster, Direction::Left, Direction::Right);
      total_consumption += left_right_consumption;
      c.power_upto_left_right_thruster = c.power_resource(left_right_consumption, total_consumption);
      // Charge battery
      let actual_power_consumption_battery = power_consumption_battery.min(c.power_upto_left_right_thruster.balance).max(0.0);
      total_consumption += power_consumption_battery;
      c.power_upto_battery = c.power_resource(power_consumption_battery, total_consumption);

      (actual_power_consumption_railgun, actual_power_consumption_jump_drive, actual_power_consumption_battery)
    };

    if self.railgun_charging {
      c.railgun_charge_time = c.railgun_capacity.map(|c| (c / actual_power_consumption_railgun) * MWH_TO_MINUTES);
    } else {
      c.railgun_charge_time = None;
    }

    c.jump_drive_charge_time = c.jump_drive_capacity.map(|c| (c / actual_power_consumption_jump_drive) * MWH_TO_MINUTES);
    if jump_power != 0.0 {
      c.jump_drive_max_distance_empty = Some((jump_power / c.total_mass_empty).min(max_jump_distance));
      c.jump_drive_max_distance_filled = Some((jump_power / c.total_mass_filled).min(max_jump_distance));
    } else {
      c.jump_drive_max_distance_empty = None;
      c.jump_drive_max_distance_filled = None;
    }

    if self.battery_mode.is_charging() {
      c.battery_charge_time = c.battery_capacity.map(|c| (c / actual_power_consumption_battery) * MWH_TO_MINUTES);
    } else {
      c.battery_charge_time = None;
    }

    {
      // Idle
      c.hydrogen_idle = c.hydrogen_resource(hydrogen_consumption_idle, hydrogen_consumption_idle);
      // Non-idle
      let mut consumption = hydrogen_consumption_engine;
      c.hydrogen_engine = c.hydrogen_resource(hydrogen_consumption_engine, consumption);
      let up_down_consumption = Self::thruster_consumption_peak(&hydrogen_consumption_thruster, Direction::Up, Direction::Down);
      consumption += up_down_consumption;
      c.hydrogen_upto_up_down_thruster = c.hydrogen_resource(up_down_consumption, consumption);
      let front_back_consumption = Self::thruster_consumption_peak(&hydrogen_consumption_thruster, Direction::Front, Direction::Back);
      consumption += front_back_consumption;
      c.hydrogen_upto_front_back_thruster = c.hydrogen_resource(front_back_consumption, consumption);
      let left_right_consumption = Self::thruster_consumption_peak(&hydrogen_consumption_thruster, Direction::Left, Direction::Right);
      consumption += left_right_consumption;
      c.hydrogen_upto_left_right_thruster = c.hydrogen_resource(left_right_consumption, consumption);
    }

    if self.hydrogen_tank_enabled {
      let hydrogen_generation = c.hydrogen_generation;
      // Assuming the tanks can accept infinite L/s.
      // TODO: should calculate the actual hydrogen generation available to tanks above, although I think tanks will gobble up all hydrogen first?
      c.hydrogen_tank_fill_time = c.hydrogen_tank_capacity.map(|c| (c / hydrogen_generation) * LS_TO_MINUTES);
    } else {
      c.hydrogen_tank_fill_time = None;
    }

    if self.hydrogen_engine_enabled {
      let hydrogen_generation = c.hydrogen_generation;
      // Assuming the engines can accept infinite L/s.
      // TODO: should calculate the actual hydrogen generation available to engines above.
      c.hydrogen_engine_fill_time = c.hydrogen_engine_capacity.map(|c| (c / hydrogen_generation) * LS_TO_MINUTES);
    } else {
      c.hydrogen_engine_fill_time = None;
    }

    c
  }

  fn thruster_consumption_peak(per_direction: &PerDirection<f64>, direction_a: Direction, direction_b: Direction) -> f64 {
    per_direction[direction_a].max(per_direction[direction_b])
  }
}


// Calculated data

#[derive(Default)]
pub struct GridCalculated {
  /// Total volume available in inventories that accept any item (L)
  pub total_volume_any: f64,
  /// Total volume available for ore in inventories that accept any item (L)
  pub total_volume_ore: f64,
  /// Total volume available for ice in inventories that accept any item (L)
  pub total_volume_ice: f64,
  /// Total volume available for ore in inventories that accept only ore (L)
  pub total_volume_ore_only: f64,
  /// Total volume available for ore in inventories that accept only ice (L)
  pub total_volume_ice_only: f64,
  /// Total mass without items (kg)
  pub total_mass_empty: f64,
  /// Total mass when fully filled with items (kg)
  pub total_mass_filled: f64,
  /// Total number of ore that can are stored
  pub total_items_ore: f64,
  /// Total number of ice that can are stored
  pub total_items_ice: f64,
  /// Total number of steel plates that can are stored
  pub total_items_steel_plate: f64,

  pub thruster_acceleration: PerDirection<ThrusterAccelerationCalculated>,
  /// Force (N)
  pub wheel_force: f64,

  /// Total power generation (MW)
  pub power_generation: f64,
  /// Idle power calculation
  pub power_idle: PowerCalculated,
  /// Railgun (charging) power calculation
  pub power_railgun: PowerCalculated,
  /// + Utility power calculation
  pub power_upto_utility: PowerCalculated,
  /// + Wheel suspension power calculation
  pub power_upto_wheel_suspension: PowerCalculated,
  /// + Jump drive (charging) power calculation
  pub power_upto_jump_drive: PowerCalculated,
  /// + Generator power calculation
  pub power_upto_generator: PowerCalculated,
  /// + Up/down thruster power calculation
  pub power_upto_up_down_thruster: PowerCalculated,
  /// + Front/back thruster power calculation
  pub power_upto_front_back_thruster: PowerCalculated,
  /// + Left/right thruster power calculation
  pub power_upto_left_right_thruster: PowerCalculated,
  /// + Battery (charging) power calculation
  pub power_upto_battery: PowerCalculated,

  /// Total power capacity in railguns (MWh), or None if there are no railguns
  pub railgun_capacity: Option<f64>,
  /// Duration until railguns are full when charging (min), or None if there are no railguns or they
  /// are not charging.
  pub railgun_charge_time: Option<f64>,

  /// Total power capacity in batteries (MWh), or None if there are no jump drives
  pub jump_drive_capacity: Option<f64>,
  /// Duration until jump drives are full when charging (min), or None if there are no jump drives
  /// or they are not charging.
  pub jump_drive_charge_time: Option<f64>,
  /// Maximum jump distance when empty (km), or None if there are no jump drives
  pub jump_drive_max_distance_empty: Option<f64>,
  /// Maximum jump distance when filled (km), or None if there are no jump drives
  pub jump_drive_max_distance_filled: Option<f64>,

  /// Total power capacity in batteries (MWh), or None if there are no batteries.
  pub battery_capacity: Option<f64>,
  /// Duration until batteries are full when charging (min), or None if there are no batteries or 
  /// they are not charging.
  pub battery_charge_time: Option<f64>,
  /// Whether batteries are discharging
  pub battery_discharging: bool,

  /// Total hydrogen generation (L/s)
  pub hydrogen_generation: f64,
  /// Idle hydrogen calculation
  pub hydrogen_idle: HydrogenCalculated,
  /// + Engine (filling) hydrogen calculation
  pub hydrogen_engine: HydrogenCalculated,
  /// + Up/down thruster hydrogen calculation
  pub hydrogen_upto_up_down_thruster: HydrogenCalculated,
  /// + Front/back thruster hydrogen calculation
  pub hydrogen_upto_front_back_thruster: HydrogenCalculated,
  /// + Left/right thruster hydrogen calculation
  pub hydrogen_upto_left_right_thruster: HydrogenCalculated,

  /// Total hydrogen capacity in tanks (L), or None if there are no hydrogen tanks.
  pub hydrogen_tank_capacity: Option<f64>,
  /// Duration until hydrogen tanks are full when refilling (min), or None if there are no hydrogen
  /// tanks or they are not refilling.
  pub hydrogen_tank_fill_time: Option<f64>,
  /// Whether hydrogen tanks are enabled
  pub hydrogen_tank_enabled: bool,

  /// Total hydrogen capacity in engines (L), or None if there are no hydrogen engines.
  pub hydrogen_engine_capacity: Option<f64>,
  /// Duration until hydrogen engines are full when refilling (min), or None if there are no hydrogen
  /// engines or they are not refilling.
  pub hydrogen_engine_fill_time: Option<f64>,
  /// Whether hydrogen engines are enabled
  pub hydrogen_engine_enabled: bool,
}

#[derive(Default)]
pub struct ThrusterAccelerationCalculated {
  /// Force (N)
  pub force: f64,
  /// Acceleration when empty and outside of gravity (m/s^2)
  pub acceleration_empty_no_gravity: Option<f64>,
  /// Acceleration when empty and inside of gravity (m/s^2)
  pub acceleration_empty_gravity: Option<f64>,
  /// Acceleration when filled and outside of gravity (m/s^2)
  pub acceleration_filled_no_gravity: Option<f64>,
  /// Acceleration when filled and outside of gravity (m/s^2)
  pub acceleration_filled_gravity: Option<f64>,
}

#[derive(Default)]
pub struct PowerCalculated {
  /// Power consumption of this group (MW)
  pub consumption: f64,
  /// Total power consumption upto this group (MW)
  pub total_consumption: f64,
  /// Power balance upto this group (+-MW)
  pub balance: f64,
  /// Duration until batteries are empty when discharging (min), or None if there are no batteries
  /// or they are not discharging.
  pub battery_duration: Option<f64>,
}

impl PowerCalculated {
  fn new(consumption: f64, total_consumption: f64, generation: f64, battery_capacity: Option<f64>, battery_discharging: bool, conversion_rate: f64) -> Self {
    let balance = generation - total_consumption;
    let battery_duration = if total_consumption != 0.0 && battery_discharging {
      battery_capacity.map(|c| (c / total_consumption) * conversion_rate)
    } else {
      None
    };
    PowerCalculated { consumption, total_consumption, balance, battery_duration }
  }
}

#[derive(Default)]
pub struct HydrogenCalculated {
  /// Hydrogen consumption of this group (L/s)
  pub consumption: f64,
  /// Total hydrogen consumption upto this group (L/s)
  pub total_consumption: f64,
  /// Hydrogen balance upto this group (+-L/s)
  pub balance: f64,
  /// Duration until hydrogen tanks are empty when discharging (min), or None if there are no 
  /// hydrogen tanks or they are stockpiling. TODO: stockpiling behaviour?
  pub tank_duration: Option<f64>,
}

impl HydrogenCalculated {
  fn new(consumption: f64, total_consumption: f64, generation: f64, tank_capacity: Option<f64>, tank_enabled: bool, conversion_rate: f64) -> Self {
    let balance = generation - total_consumption;
    let has_consumption = total_consumption != 0.0;
    let tank_duration = if has_consumption && tank_enabled {
      tank_capacity.map(|c| (c / total_consumption) * conversion_rate)
    } else {
      None
    };
    HydrogenCalculated { consumption, total_consumption, balance, tank_duration }
  }
}

impl GridCalculated {
  fn power_resource(&self, consumption: f64, total_consumption: f64) -> PowerCalculated {
    PowerCalculated::new(consumption, total_consumption, self.power_generation, self.battery_capacity, self.battery_discharging, MWH_TO_MINUTES)
  }

  fn hydrogen_resource(&self, consumption: f64, total_consumption: f64) -> HydrogenCalculated {
    HydrogenCalculated::new(consumption, total_consumption, self.hydrogen_generation, self.hydrogen_tank_capacity, self.hydrogen_tank_enabled, LS_TO_MINUTES)
  }
}
