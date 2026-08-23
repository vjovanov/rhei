    /// The general guard behind `rhei new`'s reload: a create only ever adds,
    /// so an id the project held before the write and does not hold after it is
    /// work the write destroyed.
    ///
    /// Tested at this level on purpose. Every splicing fault known today is
    /// refused one step earlier — an unbalanced ``` fence is an argument error
    /// — and the point of comparing whole id sets is that it does not need to
    /// know which fault produced the loss. So the loss is staged directly,
    /// which is the only way to exercise the guard without a bug to trigger it.
    // §FS-rhei-new.5.1 §FS-rhei-new.3.4
    mod new_verify_tests {
        use super::*;

        fn project_with_two_rheis() -> tempfile::TempDir {
            let dir = tempfile::tempdir().expect("temp dir");
            let root = dir.path();
            fs::write(root.join("index.panta.md"), "# Panta: Test\n").expect("manifest");
            for id in ["auth", "ops"] {
                fs::write(
                    root.join(format!("{id}.rhei.md")),
                    format!("# Rhei: {id}\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n"),
                )
                .expect("rhei file");
            }
            dir
        }

        #[test]
        fn the_id_set_covers_rheis_and_their_tickets() {
            let dir = project_with_two_rheis();
            let ids = create_plan_ids(dir.path()).expect("the project loads");
            for id in ["auth", "ops", "auth.1", "ops.1"] {
                assert!(ids.contains(id), "expected {id} in {ids:?}");
            }
        }

        #[test]
        fn an_unchanged_project_reports_nothing_vanished() {
            let dir = project_with_two_rheis();
            let before = create_plan_ids(dir.path()).expect("the project loads");
            assert!(vanished_ids_failure(dir.path(), Some(&before)).is_none());
        }

        #[test]
        fn an_id_that_stops_reading_back_is_named_and_undone() {
            let dir = project_with_two_rheis();
            let before = create_plan_ids(dir.path()).expect("the project loads");

            fs::remove_file(dir.path().join("ops.rhei.md")).expect("stage the loss");

            let failure =
                vanished_ids_failure(dir.path(), Some(&before)).expect("the loss must be caught");
            let said = failure.report.to_string();
            assert!(said.contains("ops.1"), "the lost ticket must be named: {said}");
            assert!(said.contains("removed ids"), "got: {said}");
            assert_eq!(failure.reason, "it removed ids that were already in the project");
        }

        /// No baseline is the honest answer for a project that did not load
        /// before the write: an empty set would read as "everything vanished".
        #[test]
        fn a_missing_baseline_accuses_the_create_of_nothing() {
            let dir = project_with_two_rheis();
            assert!(vanished_ids_failure(dir.path(), None).is_none());
        }
    }
