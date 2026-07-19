use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QoS {
    #[default]
    AtMostOnce,
    AtLeastOnce,
    ExactlyOnce,
}

impl QoS {
    pub fn at_most_once() -> Self {
        QoS::AtMostOnce
    }

    pub fn at_least_once() -> Self {
        QoS::AtLeastOnce
    }

    pub fn exactly_once() -> Self {
        QoS::ExactlyOnce
    }

    pub fn level(&self) -> u8 {
        match self {
            QoS::AtMostOnce => 0,
            QoS::AtLeastOnce => 1,
            QoS::ExactlyOnce => 2,
        }
    }

    pub fn from_level(level: u8) -> Self {
        match level {
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtMostOnce,
        }
    }

    pub fn is_at_most_once(&self) -> bool {
        matches!(self, QoS::AtMostOnce)
    }

    pub fn is_at_least_once(&self) -> bool {
        matches!(self, QoS::AtLeastOnce)
    }

    pub fn is_exactly_once(&self) -> bool {
        matches!(self, QoS::ExactlyOnce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_at_most_once_factory() {
        let qos = QoS::at_most_once();
        assert_eq!(qos, QoS::AtMostOnce);
        assert_eq!(qos.level(), 0);
        assert!(qos.is_at_most_once());
    }

    #[test]
    fn test_at_least_once_factory() {
        let qos = QoS::at_least_once();
        assert_eq!(qos, QoS::AtLeastOnce);
        assert_eq!(qos.level(), 1);
        assert!(qos.is_at_least_once());
    }

    #[test]
    fn test_exactly_once_factory() {
        let qos = QoS::exactly_once();
        assert_eq!(qos, QoS::ExactlyOnce);
        assert_eq!(qos.level(), 2);
        assert!(qos.is_exactly_once());
    }

    #[test]
    fn test_from_level_boundaries() {
        assert_eq!(QoS::from_level(0), QoS::AtMostOnce);
        assert_eq!(QoS::from_level(1), QoS::AtLeastOnce);
        assert_eq!(QoS::from_level(2), QoS::ExactlyOnce);
        assert_eq!(QoS::from_level(3), QoS::AtMostOnce);
        assert_eq!(QoS::from_level(255), QoS::AtMostOnce);
    }

    #[test]
    fn test_default_is_at_most_once() {
        let qos: QoS = Default::default();
        assert_eq!(qos, QoS::AtMostOnce);
    }
}
