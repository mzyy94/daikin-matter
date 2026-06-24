//! Matter ElectricalEnergyMeasurement cluster handler.
//!
//! Exposes Daikin's i_power totals as cumulative + periodic energy:
//! - PeriodicEnergyImported: today's Wh (week.pv[0])
//! - CumulativeEnergyImported: lifetime total (sum of year_this + year_previous)
//!   Both values are converted to mWh on the wire.

use rs_matter::dm::clusters::decl::electrical_energy_measurement as eem;
use rs_matter::dm::clusters::decl::globals::{
    MeasurementAccuracyStructBuilder, MeasurementTypeEnum,
};
use rs_matter::dm::{Cluster, Dataver, ReadContext};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{NullableBuilder, TLVBuilderParent};
use rs_matter::with;

use crate::device::Device;

pub struct EnergyHandler {
    pub(crate) dataver: Dataver,
    device: Device,
}

impl EnergyHandler {
    pub const CLUSTER: Cluster<'static> = eem::FULL_CLUSTER
        .with_revision(1)
        .with_features(
            eem::Feature::IMPORTED_ENERGY.bits()
                | eem::Feature::CUMULATIVE_ENERGY.bits()
                | eem::Feature::PERIODIC_ENERGY.bits(),
        )
        .with_attrs(with!(
            required;
            eem::AttributeId::Accuracy
            | eem::AttributeId::CumulativeEnergyImported
            | eem::AttributeId::PeriodicEnergyImported
        ))
        .with_cmds(with!());

    pub fn new(dataver: Dataver, device: Device) -> Self {
        Self { dataver, device }
    }

    fn sum_wh(list: Option<&[i32]>) -> i64 {
        list.map(|v| v.iter().map(|&x| x as i64).sum())
            .unwrap_or(0)
    }
}

impl eem::ClusterHandler for EnergyHandler {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    fn accuracy<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: MeasurementAccuracyStructBuilder<P>,
    ) -> Result<P, Error> {
        // Daikin's i_power totals are reported in whole Wh, so accuracy is ±500 mWh.
        builder
            .measurement_type(MeasurementTypeEnum::ElectricalEnergy)?
            .measured(true)?
            .min_measured_value(0)?
            .max_measured_value(i64::MAX)?
            .accuracy_ranges()?
            .push()?
            .range_min(0)?
            .range_max(i64::MAX)?
            .percent_max(None)?
            .percent_min(None)?
            .percent_typical(None)?
            .fixed_max(Some(500))?
            .fixed_min(Some(500))?
            .fixed_typical(Some(500))?
            .end()?
            .end()?
            .end()
    }

    fn cumulative_energy_imported<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: NullableBuilder<P, eem::EnergyMeasurementStructBuilder<P>>,
    ) -> Result<P, Error> {
        let status = self.device.get_status().map_err(|e| {
            warn!("Failed to get status: {e}");
            Error::from(ErrorCode::Busy)
        })?;
        let this_year = Self::sum_wh(status.power_history.year_this.get_int_list());
        let prev_year = Self::sum_wh(status.power_history.year_previous.get_int_list());
        let mwh = this_year.saturating_add(prev_year).saturating_mul(1000);
        builder
            .non_null()?
            .energy(mwh)?
            .start_timestamp(None)?
            .end_timestamp(None)?
            .start_systime(None)?
            .end_systime(None)?
            .apparent_energy(None)?
            .reactive_energy(None)?
            .end()
    }

    fn periodic_energy_imported<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: NullableBuilder<P, eem::EnergyMeasurementStructBuilder<P>>,
    ) -> Result<P, Error> {
        let status = self.device.get_status().map_err(|e| {
            warn!("Failed to get status: {e}");
            Error::from(ErrorCode::Busy)
        })?;
        // week.pv[0] is today (Wh).
        let today = status
            .power_history
            .week
            .get_int_list()
            .and_then(|v| v.first())
            .copied()
            .unwrap_or(0) as i64;
        let mwh = today.saturating_mul(1000);
        builder
            .non_null()?
            .energy(mwh)?
            .start_timestamp(None)?
            .end_timestamp(None)?
            .start_systime(None)?
            .end_systime(None)?
            .apparent_energy(None)?
            .reactive_energy(None)?
            .end()
    }
}
