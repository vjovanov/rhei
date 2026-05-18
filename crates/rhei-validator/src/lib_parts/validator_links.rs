/// Extract markdown links from a text block, returning `(display_text, target)` pairs.
fn extract_markdown_links(text: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
    re.captures_iter(text).map(|cap| (cap[1].to_string(), cap[2].to_string())).collect()
}

/// Collect all markdown links from every content field in the plan.
///
/// Returns `(location_label, display_text, target)` triples.
fn collect_all_links(rhei: &Rhei) -> Vec<(String, String, String)> {
    let mut links = Vec::new();

    for section in &rhei.content_sections {
        for (display, target) in extract_markdown_links(&section.content) {
            links.push((format!("section '{}'", section.title), display, target));
        }
    }

    for_each_node(rhei, |task| {
        for (display, target) in extract_markdown_links(&task.content) {
            let label = format!("{} {}", title_case_kind(&task.kind), task.id);
            links.push((label, display, target));
        }
    });

    links
}

/// Returns true if the link target looks like an external URL or a fragment-only anchor.
fn is_non_file_link(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
}

/// Validate that relative markdown links in all content fields point to
/// existing files, resolved against `base_path`.
fn validate_markdown_links(rhei: &Rhei, base_path: &Path, report: &mut ValidationReport) {
    let links = collect_all_links(rhei);

    for (location, display, target) in &links {
        if is_non_file_link(target) {
            continue;
        }

        // Strip fragment (e.g. "file.md#section" → "file.md")
        let file_part = target.split('#').next().unwrap_or(target);
        if file_part.is_empty() {
            continue; // pure fragment link, already handled above
        }

        let resolved = base_path.join(file_part);
        if !resolved.exists() {
            report.errors.push(format!(
                "{} contains a link [{}]({}) but '{}' does not exist",
                location, display, target, file_part
            ));
        }
    }
}

/// Detect cycles using Kahn's algorithm; report a generic cycle set on failure.
fn validate_circular_dependencies(
    _rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    // Build adjacency as dep -> dependent
    let mut nodes: HashSet<TaskId> = index.keys().cloned().collect();
    let mut adj: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    let mut indegree: HashMap<TaskId, usize> = HashMap::new();

    for n in nodes.clone() {
        adj.entry(n.clone()).or_default();
        indegree.entry(n).or_insert(0);
    }

    for task in index.values() {
        // task depends on deps; edges: dep -> task.id
        for dep in &task.prior {
            // Include unseen dependency as a node to make cycle detection robust even if integrity check was skipped.
            nodes.insert(dep.clone());
            adj.entry(dep.clone()).or_default().push(task.id.clone());
            *indegree.entry(task.id.clone()).or_insert(0) += 1;
            indegree.entry(dep.clone()).or_insert(0);
        }
    }

    // Kahn's algorithm
    let mut q: VecDeque<TaskId> =
        indegree.iter().filter_map(|(n, &d)| if d == 0 { Some(n.clone()) } else { None }).collect();
    let mut processed = 0usize;

    while let Some(n) = q.pop_front() {
        processed += 1;
        if let Some(neigh) = adj.get(&n) {
            for m in neigh {
                if let Some(d) = indegree.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        q.push_back(m.clone());
                    }
                }
            }
        }
    }

    if processed != indegree.len() {
        // Collect nodes still with indegree > 0
        let cyc_nodes: Vec<String> = indegree
            .iter()
            .filter_map(|(n, &d)| if d > 0 { Some(n.to_string()) } else { None })
            .collect();
        if !cyc_nodes.is_empty() {
            report
                .errors
                .push(format!("Circular dependency detected among tasks: {:?}", cyc_nodes));
        } else {
            report
                .errors
                .push("Circular dependency detected (unable to isolate nodes)".to_string());
        }
    }
}

// ---------------------------
// Tests
// ---------------------------
