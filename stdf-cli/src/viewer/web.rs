// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;
use std::hash::{BuildHasher, Hasher};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn page(api: &str, boot: &Value) -> CliResult<String> {
    let data = serde_json::to_string(boot)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    let html = include_str!("workspace.html")
        .replace("__VIEWER_BOOT__", &data)
        .replace("__VIEWER_API__", api)
        .replace("__VIEWER_SCRIPT__", include_str!("workspace.js"));
    Ok(crate::report_ui::decorate(html))
}
pub fn export(
    cache: &Cache,
    q: &query::Query,
    scope: &str,
    limit: usize,
    flow: Option<&str>,
) -> CliResult<String> {
    export_layout(cache, q, scope, limit, flow, None)
}
fn export_layout(
    cache: &Cache,
    q: &query::Query,
    scope: &str,
    limit: usize,
    flow: Option<&str>,
    layout: Option<&Value>,
) -> CliResult<String> {
    if !["selection", "summary"].contains(&scope) {
        return Err("invalid export scope".into());
    }
    cache.check()?;
    let mut boot = json!({"manifest":cache.manifest,"flow":flow,"offline":true,"scope":scope,"query":q,"validation":validation(cache,flow)?});
    if scope == "selection" {
        let mut input = q.clone();
        input.table = "measurements".into();
        input.filters.clear();
        input.values.clear();
        input.search.clear();
        let rows = query::all_rows(cache, &input, limit / 2)?;
        let mut evidence = BTreeMap::new();
        let mut devices = BTreeMap::new();
        let mut bytes = serde_json::to_vec(&rows)?.len();
        let mut dq = input.clone();
        dq.table = "devices".into();
        dq.tests.clear();
        query::visit(cache, &dq, false, |r| {
            if q.population(&r) {
                bytes += serde_json::to_vec(&r)?.len();
                if bytes > limit {
                    return Err("device evidence exceeds attachment budget".into());
                }
                devices.insert(r.id, r);
            }
            Ok(())
        })?;
        let runs: BTreeSet<_> = devices.values().map(|r| r.run).collect();
        let mut raw_hex = BTreeMap::new();
        let mut source = File::open(cache.root.join("source.stdf"))?;
        let mut evidence_index = store::IndexedReader::open(&cache.root, "records")?;
        let checks = if flow.is_some() {
            Some(crate::sanity::checks::Checks::load(None)?)
        } else {
            None
        };
        store::scan(cache, "records", &[], q.run, |r| {
            if devices.contains_key(&r.attempt)
                || (r.attempt == 0 && (runs.contains(&r.run) || r.run == 0))
            {
                let mut e: Evidence = evidence_index.get(r.record)?;
                if let Some(flow) = flow {
                    e.fields =
                        checks
                            .as_ref()
                            .unwrap()
                            .project(&e.kind, &flow.to_lowercase(), &e.fields);
                }
                bytes += serde_json::to_vec(&e)?.len();
                source.seek(SeekFrom::Start(e.offset))?;
                let mut raw = vec![0; e.len as usize + 4];
                source.read_exact(&mut raw)?;
                let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
                bytes += hex.len();
                raw_hex.insert(e.id, hex);
                if bytes > limit {
                    return Err("record evidence exceeds attachment budget; use summary or a smaller selection".into());
                }
                evidence.insert(e.id, e);
            }
            Ok(())
        })?;
        boot["rows"] = json!(rows);
        boot["devices"] = json!(devices.into_values().collect::<Vec<_>>());
        boot["evidence"] = json!(evidence);
        boot["raw_hex"] = json!(raw_hex);
    } else {
        let mut views = BTreeMap::new();
        for kind in [
            "histogram",
            "box",
            "probability",
            "trend",
            "scatter",
            "range",
            "pareto",
            "binmap",
            "paramap",
        ] {
            let mut p = q.clone();
            p.plot = kind.into();
            let v = match query::plot(cache, &p) {
                Ok(v) => v,
                Err(e) => json!({"unavailable":e.to_string()}),
            };
            views.insert(kind, v);
        }
        boot["plots"] = json!(views);
        let mut sq = q.clone();
        sq.table = "tests".into();
        sq.limit = 1000;
        let mut summaries = Vec::new();
        loop {
            let page = query::rows(cache, &sq)?;
            summaries.extend(page["rows"].as_array().unwrap().iter().cloned());
            if serde_json::to_vec(&summaries)?.len() > limit / 2 {
                return Err("summary exceeds attachment budget; narrow selection".into());
            }
            sq.offset += 1000;
            if sq.offset >= page["total"].as_u64().unwrap_or(0) as usize {
                break;
            }
        }
        boot["summaries"] =
            json!({"total":summaries.len(),"rows":summaries,"population":q.population});
    }
    if let Some(layout) = layout {
        let panes = layout["panes"].as_array().ok_or("invalid export layout")?;
        if panes.len() > 16 {
            return Err("too many export panes".into());
        }
        let mut saved = layout.clone();
        saved["source_hash"] = json!(cache.manifest.source_hash);
        saved["q"] = json!(q);
        boot["session"] = saved;
        if scope == "summary" {
            let mut plots = BTreeMap::new();
            for pane in panes {
                let kind = pane["kind"].as_str().unwrap_or("");
                if ["measurements", "devices", "records", "timing", "ffc"].contains(&kind) {
                    continue;
                }
                let p: query::Query = serde_json::from_value(pane["snapshot_query"].clone())?;
                let v = match query::plot(cache, &p) {
                    Ok(v) => v,
                    Err(e) => json!({"unavailable":e.to_string()}),
                };
                plots.insert(p.pane.to_string(), v);
            }
            boot["pane_plots"] = json!(plots);
        }
    }
    let html = page("", &boot)?;
    if html.len() > limit {
        return Err(format!(
            "HTML needs {} bytes, exceeds {} byte attachment budget",
            html.len(),
            limit
        )
        .into());
    }
    cache.check()?;
    Ok(html)
}
fn token() -> String {
    let mut out = String::new();
    for _ in 0..4 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(std::process::id() as u64);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}
fn respond(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) -> std::io::Result<()> {
    write!(stream,"HTTP/1.1 {status} {}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n",if status==200{"OK"}else{"Error"},body.len())?;
    stream.write_all(body)
}
pub(super) fn handle(
    stream: &mut TcpStream,
    cache: &Cache,
    args: &Arguments,
    base: &str,
    host: &str,
) -> CliResult<()> {
    // Windows accepted sockets inherit the listener's nonblocking mode. The
    // bounded request parser uses timed blocking I/O, including split packets.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut bytes = Vec::new();
    let mut b = [0; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut b)?;
        if n == 0 {
            return Err("incomplete HTTP request".into());
        }
        bytes.extend_from_slice(&b[..n]);
        if let Some(p) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            header_end = p + 4;
            break;
        }
        if bytes.len() > 16384 {
            return Err("HTTP headers too large".into());
        }
    }
    let header = std::str::from_utf8(&bytes[..header_end])?;
    let mut lines = header.split("\r\n");
    let request = lines.next().ok_or("missing request")?;
    let parts: Vec<_> = request.split_whitespace().collect();
    if parts.len() != 3 {
        return Err("invalid request".into());
    }
    let method = parts[0].to_string();
    let path = parts[1].split('?').next().unwrap().to_string();
    let headers: BTreeMap<String, String> = lines
        .filter_map(|s| s.split_once(':'))
        .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
        .collect();
    if headers.get("host").map(String::as_str) != Some(host)
        || headers
            .get("origin")
            .is_some_and(|v| v != &format!("http://{host}"))
        || !path.starts_with(base)
    {
        respond(stream, 403, "text/plain", b"Invalid viewer session")?;
        return Ok(());
    }
    if method == "GET" && path == base {
        let html = page(
            base,
            &json!({"manifest":cache.manifest,"flow":args.flow,"offline":false,"scope":"source","validation":validation(cache,args.flow.as_deref())?}),
        )?;
        respond(stream, 200, "text/html; charset=utf-8", html.as_bytes())?;
        return Ok(());
    }
    if method != "POST" || headers.contains_key("transfer-encoding") {
        return Err("expected bounded JSON POST".into());
    }
    let len = headers
        .get("content-length")
        .ok_or("missing content length")?
        .parse::<usize>()?;
    if len > 2 * 1024 * 1024 {
        return Err("request exceeds 2 MiB".into());
    }
    while bytes.len() < header_end + len {
        let n = stream.read(&mut b)?;
        if n == 0 {
            return Err("incomplete request".into());
        }
        bytes.extend_from_slice(&b[..n]);
    }
    let input: Value = serde_json::from_slice(&bytes[header_end..header_end + len])?;
    let command = path.strip_prefix(base).unwrap();
    let result = match command {
        "inspect" => {
            let id = input["id"].as_u64().ok_or("missing record ID")?;
            let mut e = cache.evidence(id)?;
            if let Some(flow) = args.flow.as_deref() {
                e.fields = crate::sanity::checks::Checks::load(None)?.project(
                    &e.kind,
                    &flow.to_lowercase(),
                    &e.fields,
                );
            }
            let mut f = File::open(cache.root.join("source.stdf"))?;
            f.seek(SeekFrom::Start(e.offset))?;
            let mut raw = vec![0; e.len as usize + 4];
            f.read_exact(&mut raw)?;
            json!({"evidence":e,"hex":raw.iter().map(|v|format!("{v:02x}")).collect::<Vec<_>>().join(" ")})
        }
        "rows" | "plot" => {
            let q: query::Query = serde_json::from_value(input)?;
            if command == "rows" {
                query::rows(cache, &q)?
            } else {
                query::plot(cache, &q)?
            }
        }
        "export" => {
            let q: query::Query = serde_json::from_value(input["query"].clone())?;
            q.validate()?;
            let html = export_layout(
                cache,
                &q,
                input["scope"].as_str().unwrap_or("selection"),
                args.export_size_mib * 1024 * 1024,
                args.flow.as_deref(),
                input.get("layout"),
            )?;
            respond(stream, 200, "text/html; charset=utf-8", html.as_bytes())?;
            return Ok(());
        }
        "distinct" => {
            let q: query::Query = serde_json::from_value(input["query"].clone())?;
            query::distinct(cache, &q, input["field"].as_str().ok_or("missing field")?)?
        }
        "data-export" => {
            let q: query::Query = serde_json::from_value(input)?;
            q.validate()?;
            json!({"source_hash":cache.manifest.source_hash,"query":q,"rows":query::all_rows(cache,&q,args.export_size_mib*1024*1024)?})
        }
        _ => return Err("unknown viewer command".into()),
    };
    let payload = serde_json::to_vec(&result)?;
    if payload.len() > cache.memory / 4 {
        return Err("query response exceeds budget; narrow selection".into());
    }
    respond(stream, 200, "application/json; charset=utf-8", &payload)?;
    Ok(())
}
pub fn serve(cache: Cache, args: &Arguments, out: &mut impl Write) -> CliResult<()> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, args.port))?;
    let host = listener.local_addr()?.to_string();
    let base = format!("/{}/", token());
    let url = format!("http://{host}{base}");
    writeln!(
        out,
        "Open {url}\nPress Ctrl+C to close the viewer. The STDF remains unchanged."
    )?;
    out.flush()?;
    if !args.no_open {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("rundll32.exe")
                .args(["url.dll,FileProtocolHandler", &url])
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(&url).spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
        }
    }
    listener.set_nonblocking(true)?;
    loop {
        cache.check()?;
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(e) = handle(&mut stream, &cache, args, &base, &host) {
                    let payload = serde_json::to_vec(&json!({"error":e.to_string()}))?;
                    let _ = respond(&mut stream, 400, "application/json", &payload);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(40))
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn validation(cache: &Cache, flow: Option<&str>) -> CliResult<Value> {
    let Some(flow) = flow else {
        return Ok(json!({"profile":null,"status":"not selected"}));
    };
    let checks = crate::sanity::checks::Checks::load(None)?;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut findings = Vec::new();
    let domain = flow.to_lowercase();
    for id in 1..=cache.manifest.records {
        cache.check()?;
        let e = cache.evidence(id)?;
        for field in checks.project(&e.kind, &domain, &e.fields) {
            *totals.entry(field.status.clone()).or_default() += 1;
            if !["valid", "not_checked"].contains(&field.status.as_str()) && findings.len() < 100 {
                findings.push(
                    json!({"record":id,"type":e.kind,"field":field.name,"status":field.status}),
                );
            }
        }
    }
    Ok(
        json!({"profile":flow.to_uppercase(),"checks_hash":checks.hash,"totals":totals,"preview":findings,"preview_limit":100,"missing_wafer_record":domain=="cp"&&!cache.manifest.inventory.contains_key("WIR"),"optional_records_required":false}),
    )
}
