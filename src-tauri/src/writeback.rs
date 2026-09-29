use super::*;

#[derive(Debug, Clone)]
struct CellChange {
    row: usize,
    column: usize,
    range: String,
    after: String,
}

fn changes(before: &[Row], after: &[Row], targets: &[String]) -> Result<Vec<CellChange>, String> {
    let mut result = Vec::new();
    for row in after {
        let original = before
            .iter()
            .find(|old| old.row_id == row.row_id)
            .ok_or("Write-back snapshot is missing a row.")?;
        if row.row_number != original.row_number || row.sheet_id != original.sheet_id {
            return Err("Write-back row identity changed.".into());
        }
        let mut seen = HashSet::new();
        if original
            .available_column_ids
            .iter()
            .any(|id| !seen.insert(id))
        {
            return Err(
                "Duplicate column ids; reload and resolve duplicate headers before writing.".into(),
            );
        }
        for id in targets {
            let Some(column) = original.available_column_ids.iter().position(|c| c == id) else {
                continue;
            };
            let old = original.values.get(id).cloned().unwrap_or_default();
            let new = row.values.get(id).cloned().unwrap_or_default();
            if old != new {
                let letter = to_excel_column_name(column);
                result.push(CellChange {
                    row: row.row_number,
                    column,
                    range: format!(
                        "{}!{}{}:{}{}",
                        row.sheet_id, letter, row.row_number, letter, row.row_number
                    ),
                    after: new,
                });
            }
        }
    }
    Ok(result)
}

fn check_snapshot(rows: &[Row], live: &[Vec<String>]) -> Result<(), String> {
    let headers = live
        .first()
        .ok_or("Sheet headers missing before write-back.")?;
    let ids: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| normalize_column_id(h, i))
        .collect();
    for row in rows {
        if ids != row.available_column_ids {
            return Err("Sheet columns changed; reload before writing.".into());
        }
        let current = live
            .get(row.row_number.saturating_sub(1))
            .ok_or("Sheet row moved or was deleted.")?;
        for (column, id) in ids.iter().enumerate() {
            let actual = current.get(column).map(String::as_str).unwrap_or("");
            let expected = row.values.get(id).map(String::as_str).unwrap_or("");
            if actual != expected {
                return Err(format!("Write conflict at {}!{}{}; reload the sheet. No overwrite attempted for this sheet.",
                    row.sheet_id, to_excel_column_name(column), row.row_number));
            }
        }
    }
    Ok(())
}

fn verify(changes: &[CellChange], live: &[Vec<String>]) -> Result<(), String> {
    for change in changes {
        let actual = live
            .get(change.row.saturating_sub(1))
            .and_then(|r| r.get(change.column))
            .map(String::as_str)
            .unwrap_or("");
        if actual != change.after {
            return Err(format!("Write-back verification failed at {}. Reload before retrying; remote data may have changed.", change.range));
        }
    }
    Ok(())
}

async fn read_sheet(
    client: &Client,
    root: &str,
    token: &str,
    sheet: &str,
) -> Result<Vec<Vec<String>>, String> {
    let response = client
        .get(format!("{}/values/{}", root, urlencoding::encode(sheet)))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Write-back read failed: {e}"))?;
    let json = parse_feishu_json_response(response, "Write-back read failed").await?;
    let values = json
        .pointer("/data/valueRange/values")
        .and_then(Value::as_array)
        .ok_or("Write-back read missing values.")?;
    values
        .iter()
        .map(|row| {
            row.as_array()
                .map(|cells| cells.iter().map(json_value_to_string).collect())
                .ok_or_else(|| "Invalid sheet row.".to_string())
        })
        .collect()
}

pub(super) async fn sync(
    client: &Client,
    token: &str,
    spreadsheet: &str,
    before: &[Row],
    after: &[Row],
    targets: &[String],
) -> Result<(), String> {
    let root = format!("https://open.feishu.cn/open-apis/sheets/v2/spreadsheets/{spreadsheet}");
    sync_at(client, token, &root, before, after, targets).await
        .map_err(|e|format!("Write-back not fully verified: {e} Earlier batches may already be saved; reload before retrying."))
}

async fn sync_at(
    client: &Client,
    token: &str,
    root: &str,
    before: &[Row],
    after: &[Row],
    targets: &[String],
) -> Result<(), String> {
    let sheets: HashSet<_> = after.iter().map(|r| r.sheet_id.clone()).collect();
    for sheet in sheets {
        ensure_task_not_cancelled("translation")?;
        let updated: Vec<_> = after
            .iter()
            .filter(|r| r.sheet_id == sheet)
            .cloned()
            .collect();
        let ids: HashSet<_> = updated.iter().map(|r| r.row_id.as_str()).collect();
        let mut original: Vec<_> = before
            .iter()
            .filter(|r| ids.contains(r.row_id.as_str()))
            .cloned()
            .collect();
        let mutations = changes(&original, &updated, targets)?;
        if mutations.is_empty() {
            continue;
        }
        for batch in mutations.chunks(100) {
            ensure_task_not_cancelled("translation")?;
            let live = read_sheet(client, root, token, &sheet).await?;
            check_snapshot(&original, &live)?;
            ensure_task_not_cancelled("translation")?;
            let values: Vec<_> = batch
                .iter()
                .map(|c| json!({"range":c.range,"values":[[c.after]]}))
                .collect();
            let response = client
                .post(format!("{root}/values_batch_update"))
                .bearer_auth(token)
                .json(&json!({"valueRanges":values}))
                .send()
                .await
                .map_err(|e| format!("Write-back request failed: {e}"))?;
            parse_feishu_json_response(response, "Write-back failed").await?;
            // Never cancel an in-flight write before checking what reached the server.
            let live = read_sheet(client, root, token, &sheet).await?;
            verify(batch, &live)?;
            for change in batch {
                if let Some(row) = original.iter_mut().find(|r| r.row_number == change.row) {
                    row.values.insert(
                        row.available_column_ids[change.column].clone(),
                        change.after.clone(),
                    );
                }
            }
            check_snapshot(&original, &live)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Row {
        Row {
            row_id: "key1".into(),
            row_number: 2,
            sheet_id: "sheet".into(),
            sheet_name: "Sheet".into(),
            available_column_ids: vec!["key".into(), "english".into(), "japanese".into()],
            values: HashMap::from([
                ("key".into(), "key1".into()),
                ("english".into(), "".into()),
                ("japanese".into(), "既存".into()),
            ]),
        }
    }
    fn live(english: &str) -> Vec<Vec<String>> {
        vec![
            vec!["Key".into(), "English".into(), "Japanese".into()],
            vec!["key1".into(), english.into(), "既存".into()],
        ]
    }
    #[test]
    fn only_changed_cells_are_written_and_conflicts_fail() {
        let original = row();
        let mut after = original.clone();
        after.values.insert("english".into(), "Ant".into());
        let diff = changes(
            &[original.clone()],
            &[after],
            &["english".into(), "japanese".into()],
        )
        .unwrap();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].range, "sheet!B2:B2");
        assert!(check_snapshot(&[original.clone()], &live("")).is_ok());
        assert!(check_snapshot(&[original], &live("someone edited")).is_err());
        assert!(verify(&diff, &live("Ant")).is_ok());
        assert!(verify(&diff, &live("")).is_err());
    }

    #[tokio::test]
    async fn mock_server_checks_exact_write_and_readback() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut bytes = [0; 4096];
                loop {
                    let n = socket.read(&mut bytes).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&bytes[..n]);
                    if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..i]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .unwrap_or("0")
                            .parse()
                            .unwrap();
                        if request.len() >= i + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(request).unwrap();
                let body=if step==1 {
                    assert!(request.starts_with("POST /values_batch_update"));
                    let payload:Value=serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                    assert_eq!(payload["valueRanges"].as_array().unwrap().len(),1);
                    assert_eq!(payload["valueRanges"][0]["range"],"sheet!B2:B2");
                    json!({"code":0})
                } else {
                    assert!(request.starts_with("GET /values/sheet"));
                    json!({"code":0,"data":{"valueRange":{"values":live(if step==2 {"Ant"}else{""})}}})
                }.to_string();
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        let original = row();
        let mut after = original.clone();
        after.values.insert("english".into(), "Ant".into());
        sync_at(
            &build_http_client().unwrap(),
            "test",
            &format!("http://{address}"),
            &[original],
            &[after],
            &["english".into(), "japanese".into()],
        )
        .await
        .unwrap();
        server.join().unwrap();
    }
}
