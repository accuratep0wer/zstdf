use crate::{error::Result, fields::FieldReader};

/// Advantest V93000 Activity Trace Event Record (137, 10).
/// The wire layout follows the supplied SmarTest 8.8.2 field table.
#[derive(Debug, Clone)]
pub struct Ater {
    pub rec_custm: String,
    pub evt_src: String,
    pub head_num: u8,
    pub site_num: u8,
    pub activity: String,
    /// Exact C*n payload, including non-text bytes from binary activity logging.
    pub activity_bytes: Vec<u8>,
}

impl Ater {
    pub fn parse(r: &mut FieldReader) -> Result<Self> {
        let rec_custm = r.read_cn()?;
        let evt_src = r.read_cn()?;
        let head_num = r.read_u1()?;
        let site_num = r.read_u1()?;
        // B*n and C*n have identical byte-length framing. Keep the original payload.
        let activity_bytes = r.read_bn()?;
        let activity = String::from_utf8_lossy(&activity_bytes).into_owned();
        Ok(Self {
            rec_custm,
            evt_src,
            head_num,
            site_num,
            activity,
            activity_bytes,
        })
    }
}
