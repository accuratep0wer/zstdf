use super::*;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::io::{Seek, SeekFrom};
use std::rc::Rc;
use stdf_arrow::identity::{wafer_key, CoordinateCandidates};
use stdf_core::{RecordType, StdfRecord};
use stdf_io::StreamingRecordReader;

fn open(path: &Path) -> CliResult<Box<dyn Read>> {
    let mut file = File::open(path)?;
    let mut magic = [0; 2];
    file.read_exact(&mut magic)?;
    file.seek(SeekFrom::Start(0))?;
    if magic == [0x1f, 0x8b] {
        Ok(Box::new(flate2::read::MultiGzDecoder::new(file)))
    } else {
        Ok(Box::new(file))
    }
}
pub fn hash(path: &Path, budget: &Budget) -> CliResult<String> {
    let mut reader = open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        budget.check()?;
        let n = reader.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
struct HashReader {
    reader: Box<dyn Read>,
    digest: Rc<RefCell<Sha256>>,
}
impl Read for HashReader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(bytes)?;
        self.digest.borrow_mut().update(&bytes[..n]);
        Ok(n)
    }
}
type Site = (u8, u8);
type Hardware = BTreeMap<String, Option<String>>;
struct OpenPart {
    coordinates: CoordinateCandidates,
    sequence: u64,
    wafer: Option<String>,
    hardware: Hardware,
}
fn wafer(
    head: u8,
    site: u8,
    sites: &BTreeMap<Site, (u8, Hardware)>,
    wafers: &BTreeMap<Site, Option<String>>,
) -> Option<String> {
    if let Some((group, _)) = sites.get(&(head, site)) {
        return wafers
            .get(&(head, *group))
            .or_else(|| wafers.get(&(head, 255)))
            .cloned()
            .flatten();
    }
    if let Some(w) = wafers.get(&(head, 255)) {
        return w.clone();
    }
    let mut candidates = wafers.iter().filter(|((h, _), _)| *h == head);
    let first = candidates.next()?.1.clone();
    if candidates.next().is_none() {
        first
    } else {
        None
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read(
    path: &Path,
    hash: &str,
    flow: &Flow,
    aliases: &BTreeMap<String, String>,
    runs: &mut BTreeMap<String, Run>,
    store: &mut SpillStore,
    budget: &mut Budget,
) -> CliResult<()> {
    let digest = Rc::new(RefCell::new(Sha256::new()));
    let mut reader = StreamingRecordReader::new(HashReader {
        reader: open(path)?,
        digest: digest.clone(),
    })?;
    let mut sites: BTreeMap<Site, (u8, Hardware)> = BTreeMap::new();
    let mut wafers: BTreeMap<Site, Option<String>> = BTreeMap::new();
    let mut active: BTreeMap<Site, OpenPart> = BTreeMap::new();
    let mut sequence = 0u64;
    let mut run_number = 0u64;
    let mut run_key: Option<String> = None;
    let mut lot = String::new();
    let mut step = None;
    loop {
        budget.check()?;
        let offset = reader.bytes_consumed();
        let Some(record) = reader.next() else {
            break;
        };
        let record =
            record.map_err(|e| format!("{} at decompressed byte {offset}: {e}", path.display()))?;
        match record {
            StdfRecord::Unknown { typ, sub, .. } => {
                if !matches!(
                    RecordType::from_type_sub(typ, sub),
                    RecordType::Unknown(_, _)
                ) {
                    return Err(format!(
                        "malformed record ({typ},{sub}) at {}:{offset}",
                        path.display()
                    )
                    .into());
                }
            }
            StdfRecord::Mir(mir) => {
                if !active.is_empty() || run_key.is_some() {
                    return Err("MIR before closing prior run/parts".into());
                }
                sites.clear();
                wafers.clear();
                lot = mir.lot_id.clone();
                let program = Selector {
                    job_nam: Some(mir.job_nam.clone()),
                    job_rev: mir.job_rev.clone(),
                    test_cod: mir.test_cod.clone(),
                    flow_id: mir.flow_id.clone(),
                    tst_temp: mir.tst_temp.clone(),
                };
                step = flow.match_step(&program);
                run_number += 1;
                let key = format!("{hash}:{run_number}");
                let mut fields = BTreeMap::new();
                macro_rules! capture { ($($f:ident),* $(,)?) => { $(fields.insert(stringify!($f).into(), serde_json::json!(mir.$f));)* }; }
                capture!(
                    setup_t, start_t, stat_num, mode_cod, rtst_cod, prot_cod, burn_tim, cmod_cod,
                    lot_id, part_typ, node_nam, tstr_typ, job_nam, job_rev, sblot_id, oper_nam,
                    exec_typ, exec_ver, test_cod, tst_temp, user_txt, aux_file, pkg_typ, famly_id,
                    date_cod, facil_id, floor_id, proc_id, oper_frq, spec_nam, spec_ver, flow_id,
                    setup_id, dsgn_rev, eng_id, rom_cod, serl_num, supr_nam
                );
                let run = Run {
                    program,
                    start: (mir.start_t != 0).then_some(mir.start_t),
                    finish: None,
                    mir: fields,
                };
                budget.charge(serde_json::to_vec(&run)?.len() * 4 + 1024)?;
                runs.insert(key.clone(), run);
                run_key = Some(key);
            }
            StdfRecord::Mrr(mrr) => {
                if !active.is_empty() {
                    return Err("MRR with unclosed test instances".into());
                }
                let key = run_key.take().ok_or("MRR without MIR")?;
                runs.get_mut(&key)
                    .ok_or("MRR does not match an open MIR")?
                    .finish = (mrr.finish_t != 0).then_some(mrr.finish_t);
            }
            StdfRecord::Sdr(sdr) => {
                let mut hardware = BTreeMap::new();
                macro_rules! capture { ($($f:ident),* $(,)?) => { $(hardware.insert(stringify!($f).into(), sdr.$f.clone());)* }; }
                capture!(
                    hand_typ, hand_id, card_typ, card_id, load_typ, load_id, dib_typ, dib_id,
                    cabl_typ, cabl_id, cont_typ, cont_id, lasr_typ, lasr_id, extr_typ, extr_id
                );
                for site in sdr.site_num {
                    sites.insert((sdr.head_num, site), (sdr.site_grp, hardware.clone()));
                }
            }
            StdfRecord::Wir(wir) => {
                wafers.insert((wir.head_num, wir.site_grp.unwrap_or(255)), wir.wafer_id);
            }
            StdfRecord::Wrr(wrr) => {
                wafers.remove(&(wrr.head_num, wrr.site_grp));
            }
            StdfRecord::Pir(pir) => {
                if run_key.is_none() {
                    return Err("PIR outside MIR/MRR run".into());
                }
                let site = (pir.head_num, pir.site_num);
                if active.contains_key(&site) {
                    return Err("PIR replaces an unclosed test instance".into());
                }
                sequence += 1;
                active.insert(
                    site,
                    OpenPart {
                        sequence,
                        coordinates: CoordinateCandidates::default(),
                        wafer: wafer(site.0, site.1, &sites, &wafers),
                        hardware: sites.get(&site).map(|s| s.1.clone()).unwrap_or_default(),
                    },
                );
            }
            StdfRecord::Ptr(ptr) => {
                let part = active
                    .get_mut(&(ptr.head_num, ptr.site_num))
                    .ok_or("PTR without matching head/site PIR")?;
                if let Some(name) = &ptr.test_txt {
                    part.coordinates.observe(name, Some(ptr.result));
                }
            }
            StdfRecord::Mpr(mpr) => {
                if !active.contains_key(&(mpr.head_num, mpr.site_num)) {
                    return Err("MPR without matching head/site PIR".into());
                }
            }
            StdfRecord::Ftr(ftr) => {
                if !active.contains_key(&(ftr.head_num, ftr.site_num)) {
                    return Err("FTR without matching head/site PIR".into());
                }
            }
            StdfRecord::Prr(prr) => {
                let run = run_key.as_ref().ok_or("PRR outside MIR/MRR run")?.clone();
                let site = (prr.head_num, prr.site_num);
                let part = active.remove(&site).unwrap_or_else(|| {
                    sequence += 1;
                    OpenPart {
                        sequence,
                        coordinates: CoordinateCandidates::default(),
                        wafer: wafer(site.0, site.1, &sites, &wafers),
                        hardware: sites.get(&site).map(|s| s.1.clone()).unwrap_or_default(),
                    }
                });
                let original_identity = wafer_key(part.wafer.as_deref(), prr.x_coord, prr.y_coord)
                    .or_else(|| part.coordinates.lot_key(&lot));
                let identity = original_identity
                    .as_ref()
                    .map(|k| aliases.get(k).unwrap_or(k).clone());
                let key = identity
                    .clone()
                    .unwrap_or_else(|| format!("unresolved:{hash}:{}", part.sequence));
                let attempt = Attempt {
                    source: hash.into(),
                    run,
                    offset,
                    sequence: part.sequence,
                    step,
                    identity,
                    original_identity,
                    lot: lot.clone(),
                    wafer: part.wafer,
                    prr_x: prr.x_coord,
                    prr_y: prr.y_coord,
                    head: prr.head_num,
                    site: prr.site_num,
                    part_id: prr.part_id,
                    part_flg: prr.part_flg,
                    passed: (prr.part_flg & 0x14 == 0).then_some(prr.part_flg & 0x08 == 0),
                    hard_bin: (prr.hard_bin <= 32767).then_some(prr.hard_bin),
                    soft_bin: (prr.soft_bin <= 32767).then_some(prr.soft_bin),
                    raw_hard_bin: prr.hard_bin,
                    raw_soft_bin: prr.soft_bin,
                    hardware: part.hardware,
                };
                store.push(key.as_bytes(), &serde_json::to_vec(&attempt)?)?;
            }
            _ => {}
        }
        if (active.len() + sites.len()) * 8192 + wafers.len() * 1024 > budget.metadata_limit / 2 {
            return Err("active head/site metadata memory limit exceeded".into());
        }
    }
    if !active.is_empty() {
        return Err(format!("unclosed test instances at EOF: {}", path.display()).into());
    }
    if run_key.is_some() {
        return Err("unclosed STDF run at EOF (missing MRR)".into());
    }
    if run_number == 0 {
        return Err("input has no MIR run".into());
    }
    let actual = format!("{:x}", digest.borrow().clone().finalize());
    if actual != hash {
        return Err("STDF content changed between hash and parse passes".into());
    }
    Ok(())
}
