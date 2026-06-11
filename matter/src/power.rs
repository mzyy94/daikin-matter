use rs_matter::dm::clusters::decl::electrical_power_measurement;
use rs_matter::dm::clusters::decl::globals::{
    MeasurementAccuracyStructArrayBuilder, MeasurementAccuracyStructBuilder, MeasurementTypeEnum,
};
use rs_matter::dm::{ArrayAttributeRead, Cluster, Dataver, ReadContext};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::im::PowerMilliW;
use rs_matter::tlv::{Nullable, TLVBuilderParent};
use rs_matter::with;

use electrical_power_measurement::PowerModeEnum;

use crate::device::Device;

pub struct PowerHandler {
    pub(crate) dataver: Dataver,
    device: Device,
}

impl PowerHandler {
    pub const CLUSTER: Cluster<'static> = electrical_power_measurement::FULL_CLUSTER
        .with_revision(1)
        .with_features(electrical_power_measurement::Feature::ALTERNATING_CURRENT.bits())
        .with_attrs(with!(
            required;
            electrical_power_measurement::AttributeId::ActivePower
        ))
        .with_cmds(with!());

    pub fn new(dataver: Dataver, device: Device) -> Self {
        Self { dataver, device }
    }
}

impl electrical_power_measurement::ClusterHandler for PowerHandler {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    fn power_mode(&self, _ctx: impl ReadContext) -> Result<PowerModeEnum, Error> {
        Ok(PowerModeEnum::AC)
    }

    fn number_of_measurement_types(&self, _ctx: impl ReadContext) -> Result<u8, Error> {
        Ok(1)
    }

    fn accuracy<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: ArrayAttributeRead<
            MeasurementAccuracyStructArrayBuilder<P>,
            MeasurementAccuracyStructBuilder<P>,
        >,
    ) -> Result<P, Error> {
        match builder {
            ArrayAttributeRead::ReadAll(array) => array
                .push()?
                .measurement_type(MeasurementTypeEnum::ActivePower)?
                .measured(true)?
                .min_measured_value(0)?
                .max_measured_value(4_000_000)?
                .accuracy_ranges()?
                .push()?
                .range_min(0)?
                .range_max(4_000_000)?
                .percent_max(None)?
                .percent_min(None)?
                .percent_typical(None)?
                .fixed_max(Some(10_000))?
                .fixed_min(Some(10_000))?
                .fixed_typical(Some(10_000))?
                .end()?
                .end()?
                .end()?
                .end(),
            ArrayAttributeRead::ReadOne(0, elem) => elem
                .measurement_type(MeasurementTypeEnum::ActivePower)?
                .measured(true)?
                .min_measured_value(0)?
                .max_measured_value(4_000_000)?
                .accuracy_ranges()?
                .push()?
                .range_min(0)?
                .range_max(4_000_000)?
                .percent_max(None)?
                .percent_min(None)?
                .percent_typical(None)?
                .fixed_max(Some(10_000))?
                .fixed_min(Some(10_000))?
                .fixed_typical(Some(10_000))?
                .end()?
                .end()?
                .end(),
            ArrayAttributeRead::ReadOne(_, _) => Err(ErrorCode::ConstraintError.into()),
            ArrayAttributeRead::ReadNone(array) => array.end(),
        }
    }

    fn active_power(&self, _ctx: impl ReadContext) -> Result<Nullable<PowerMilliW>, Error> {
        let status = self.device.get_status().map_err(|e| {
            warn!("Failed to get status: {e}");
            Error::from(ErrorCode::Busy)
        })?;
        match status.power_consumption.get_f32() {
            Some(watts) => Ok(Nullable::some((watts * 1000.0) as i64)),
            None => Ok(Nullable::none()),
        }
    }
}
