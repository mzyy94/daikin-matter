use rs_matter::dm::clusters::decl::power_topology;
use rs_matter::dm::{Cluster, Dataver};
use rs_matter::with;

pub struct PowerTopologyHandler {
    pub(crate) dataver: Dataver,
}

impl PowerTopologyHandler {
    pub const CLUSTER: Cluster<'static> = power_topology::FULL_CLUSTER
        .with_revision(1)
        .with_features(power_topology::Feature::NODE_TOPOLOGY.bits())
        .with_attrs(with!(required))
        .with_cmds(with!());

    pub fn new(dataver: Dataver) -> Self {
        Self { dataver }
    }
}

impl power_topology::ClusterHandler for PowerTopologyHandler {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }
    fn dataver_changed(&self) {
        self.dataver.changed();
    }
}
