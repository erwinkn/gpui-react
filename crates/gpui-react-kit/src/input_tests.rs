use super::*;

fn has_binding(bindings: &[KeyBinding], keystroke: &str, action: &dyn gpui::Action) -> bool {
    let keystroke = gpui::Keystroke::parse(keystroke).unwrap();
    bindings.iter().any(|binding| {
        binding.match_keystrokes(std::slice::from_ref(&keystroke)) == Some(false)
            && binding.action().partial_eq(action)
    })
}

#[test]
fn macos_word_navigation_uses_alt() {
    let bindings = text_editor_bindings(INPUT_KEY_CONTEXT, false, true, true, true);

    assert!(has_binding(&bindings, "alt-left", &WordLeft));
    assert!(has_binding(&bindings, "alt-right", &WordRight));
    assert!(!has_binding(&bindings, "ctrl-left", &WordLeft));
    assert!(!has_binding(&bindings, "ctrl-right", &WordRight));
}

#[test]
fn non_macos_word_navigation_uses_control() {
    let bindings = text_editor_bindings(INPUT_KEY_CONTEXT, false, true, false, true);

    assert!(has_binding(&bindings, "ctrl-left", &WordLeft));
    assert!(has_binding(&bindings, "ctrl-right", &WordRight));
    assert!(!has_binding(&bindings, "alt-left", &WordLeft));
    assert!(!has_binding(&bindings, "alt-right", &WordRight));
}

#[test]
fn browser_paste_stays_with_the_dom_event() {
    let bindings = text_editor_bindings(INPUT_KEY_CONTEXT, false, true, true, false);

    assert!(!has_binding(&bindings, "cmd-v", &Paste));
    assert!(!has_binding(&bindings, "ctrl-v", &Paste));
}

#[test]
fn desktop_paste_uses_the_platform_clipboard_action() {
    let bindings = text_editor_bindings(INPUT_KEY_CONTEXT, false, true, true, true);

    assert!(has_binding(&bindings, "cmd-v", &Paste));
    assert!(has_binding(&bindings, "ctrl-v", &Paste));
}

#[test]
fn textarea_enter_inserts_a_newline_unless_on_submit_is_set() {
    let textarea = text_editor_bindings(TEXTAREA_KEY_CONTEXT, true, false, true, true);
    assert!(has_binding(&textarea, "enter", &Newline));
    assert!(has_binding(&textarea, "shift-enter", &Newline));
    assert!(!has_binding(&textarea, "enter", &Submit));

    let composer = text_editor_bindings(TEXTAREA_SUBMIT_KEY_CONTEXT, true, true, true, true);
    assert!(has_binding(&composer, "enter", &Submit));
    assert!(has_binding(&composer, "shift-enter", &Newline));
    assert!(!has_binding(&composer, "enter", &Newline));

    let input = text_editor_bindings(INPUT_KEY_CONTEXT, false, true, true, true);
    assert!(has_binding(&input, "enter", &Submit));
    assert!(!has_binding(&input, "enter", &Newline));
}

#[test]
fn ime_offsets_are_relative_to_replacement_text() {
    assert_eq!(utf16_offset_to_utf8("é🙂", 0), 0);
    assert_eq!(utf16_offset_to_utf8("é🙂", 1), "é".len());
    assert_eq!(utf16_offset_to_utf8("é🙂", 3), "é🙂".len());
}

#[test]
fn single_line_newlines_become_one_space() {
    assert_eq!(single_line_text("a\r\nb\nc\rd"), "a b c d");
}

#[test]
fn caret_blink_phase() {
    assert!(caret_visible(0));
    assert!(caret_visible(CARET_BLINK_MS - 1));
    assert!(!caret_visible(CARET_BLINK_MS));
    assert!(!caret_visible(2 * CARET_BLINK_MS - 1));
    assert!(caret_visible(2 * CARET_BLINK_MS));
}

#[test]
fn caret_matches_the_font_size_inside_the_line() {
    let bounds = caret_rect(point(px(10.0), px(4.0)), px(20.0), px(16.0));
    assert_eq!(bounds.origin, point(px(10.0), px(8.0)));
    assert_eq!(bounds.size, size(px(2.0), px(12.0)));
    assert_eq!(
        caret_rect(point(px(0.0), px(0.0)), px(20.0), px(40.0))
            .size
            .height,
        px(20.0)
    );
}

#[test]
fn insertion_undo_coalescing_requires_one_contiguous_non_whitespace_character() {
    let insert_at_one = CoalescingEdit {
        kind: EditKind::Insert,
        anchor: 1,
    };

    assert_eq!(coalescing_edit(&(0..0), "a", false), Some(insert_at_one));
    assert!(edits_coalesce(
        insert_at_one,
        coalescing_edit(&(1..1), "b", false),
        &(1..1),
        Duration::from_millis(699),
    ));
    assert!(!edits_coalesce(
        insert_at_one,
        coalescing_edit(&(2..2), "b", false),
        &(2..2),
        Duration::from_millis(699),
    ));
    assert_eq!(coalescing_edit(&(0..1), "a", false), None);
    assert_eq!(coalescing_edit(&(1..1), "ab", false), None);
    assert_eq!(coalescing_edit(&(1..1), " ", false), None);
    assert_eq!(coalescing_edit(&(1..1), "\n", false), None);
    assert_eq!(coalescing_edit(&(1..1), "\t", false), None);
    assert_eq!(coalescing_edit(&(1..1), "\u{2003}", false), None);
    assert!(!edits_coalesce(
        insert_at_one,
        coalescing_edit(&(1..1), "b", false),
        &(1..1),
        UNDO_COALESCE,
    ));
    assert!(!edits_coalesce(
        CoalescingEdit {
            kind: EditKind::DeleteBackward,
            anchor: 1,
        },
        coalescing_edit(&(1..1), "b", false),
        &(1..1),
        Duration::from_millis(1),
    ));
    assert!(!edits_coalesce(
        insert_at_one,
        None,
        &(1..1),
        Duration::from_millis(1),
    ));
}

#[test]
fn backward_and_forward_deletions_use_their_own_contiguity_rules() {
    let backward = CoalescingEdit {
        kind: EditKind::DeleteBackward,
        anchor: 3,
    };
    assert_eq!(
        coalescing_edit(&(2..3), "", true),
        Some(CoalescingEdit {
            kind: EditKind::DeleteBackward,
            anchor: 2,
        })
    );
    assert!(edits_coalesce(
        backward,
        coalescing_edit(&(2..3), "", true),
        &(2..3),
        Duration::from_millis(699),
    ));
    assert!(!edits_coalesce(
        backward,
        coalescing_edit(&(1..2), "", true),
        &(1..2),
        Duration::from_millis(699),
    ));

    let forward = CoalescingEdit {
        kind: EditKind::DeleteForward,
        anchor: 2,
    };
    assert_eq!(coalescing_edit(&(2..3), "", false), Some(forward));
    assert!(edits_coalesce(
        forward,
        coalescing_edit(&(2..3), "", false),
        &(2..3),
        Duration::from_millis(699),
    ));
    assert!(!edits_coalesce(
        forward,
        coalescing_edit(&(3..4), "", false),
        &(3..4),
        Duration::from_millis(699),
    ));
    assert!(!edits_coalesce(
        forward,
        coalescing_edit(&(2..3), "", false),
        &(2..3),
        UNDO_COALESCE,
    ));
    assert_eq!(coalescing_edit(&(2..2), "", false), None);
}

#[test]
fn undo_history_discards_only_the_oldest_snapshot_at_the_limit() {
    let mut history = VecDeque::new();
    for index in 0..=UNDO_LIMIT {
        push_undo_snapshot(
            &mut history,
            EditSnapshot {
                content: index.to_string(),
                selected_range: index..index,
                selection_reversed: false,
            },
        );
    }

    assert_eq!(history.len(), UNDO_LIMIT);
    assert_eq!(history.front().unwrap().content, "1");
    assert_eq!(history.back().unwrap().content, UNDO_LIMIT.to_string());
}

#[test]
fn drag_autoscroll_is_edge_proportional_and_capped_to_one_line() {
    let line_height = 20.0;
    assert_eq!(drag_scroll_delta(200.0, 100.0, 300.0, line_height), 0.0);
    assert_eq!(drag_scroll_delta(90.0, 100.0, 300.0, line_height), -2.0);
    assert_eq!(drag_scroll_delta(315.0, 100.0, 300.0, line_height), 3.0);
    assert_eq!(drag_scroll_delta(-100.0, 100.0, 300.0, line_height), -20.0);
    assert_eq!(drag_scroll_delta(500.0, 100.0, 300.0, line_height), 20.0);
}

#[test]
fn multi_click_selects_word_then_all_and_does_not_arm_drag() {
    assert_eq!(press_intent(1, false), PressIntent::PlaceCaret);
    assert_eq!(press_intent(1, true), PressIntent::ExtendSelection);
    assert_eq!(press_intent(2, false), PressIntent::SelectWord);
    assert_eq!(press_intent(2, true), PressIntent::SelectWord);
    assert_eq!(press_intent(3, false), PressIntent::SelectAll);
    assert!(press_intent(1, false).arms_drag());
    assert!(press_intent(1, true).arms_drag());
    assert!(!press_intent(2, false).arms_drag());
    assert!(!press_intent(3, false).arms_drag());
}

fn with_input(
    cx: &mut gpui::TestAppContext,
    props: InputProps,
    test: impl FnOnce(&mut Input, &mut Window, &mut Context<Input>),
) {
    let window = cx.add_window(|_, _| gpui::Empty);
    window
        .update(cx, |_, window, cx| {
            let input = cx.new(|cx| Input::new(props, cx));
            input.update(cx, |input, cx| test(input, window, cx));
        })
        .unwrap();
}

#[gpui::test]
fn delayed_replacement_cannot_erase_a_newer_native_edit(cx: &mut gpui::TestAppContext) {
    with_input(cx, InputProps::default(), |input, window, cx| {
        input.replace_text_in_range(None, "a", window, cx);
        let old = input.current_snapshot();
        input.replace_text_in_range(None, "b", window, cx);
        let result = input.apply_command(
            InputCommand::Replace {
                value: "A".into(),
                expected_revision: old.revision,
            },
            window,
            cx,
        );
        assert!(result.unwrap_err().to_string().contains("stale"));
        assert_eq!(input.content, "ab");
        input
            .apply_command(
                InputCommand::Replace {
                    value: "AB".into(),
                    expected_revision: input.revision,
                },
                window,
                cx,
            )
            .unwrap();
        assert_eq!(input.content, "AB");
        input.undo(&Undo, window, cx);
        assert_eq!(input.content, "ab");
    });
}

#[gpui::test]
fn delayed_props_preserve_typing_selection_and_undo(cx: &mut gpui::TestAppContext) {
    with_input(cx, InputProps::default(), |input, window, cx| {
        input.replace_text_in_range(None, "hello", window, cx);
        input.left(&Left, window, cx);
        let revision = input.revision;
        input.update_props(
            InputProps {
                initial_value: "stale echo".into(),
                placeholder: "new".into(),
                ..Default::default()
            },
            cx,
        );
        assert_eq!(input.content, "hello");
        assert_eq!(input.selected_range, 4..4);
        assert_eq!(input.revision, revision);
        input.undo(&Undo, window, cx);
        assert_eq!(input.content, "");
    });
}

#[gpui::test]
fn composition_is_native_and_one_undo_step(cx: &mut gpui::TestAppContext) {
    with_input(
        cx,
        InputProps {
            initial_value: "x".into(),
            ..Default::default()
        },
        |input, window, cx| {
            input.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
            assert_eq!(input.content, "xni");
            assert_eq!(input.marked_text_range(window, cx), Some(1..3));
            assert!(
                input
                    .apply_command(
                        InputCommand::Replace {
                            value: "late".into(),
                            expected_revision: input.revision
                        },
                        window,
                        cx
                    )
                    .unwrap_err()
                    .to_string()
                    .contains("composition")
            );
            input.replace_and_mark_text_in_range(None, "你", Some(1..1), window, cx);
            input.replace_text_in_range(None, "你", window, cx);
            assert_eq!(input.content, "x你");
            assert!(input.marked_range.is_none());
            input.undo(&Undo, window, cx);
            assert_eq!(input.content, "x");
            input.redo(&Redo, window, cx);
            assert_eq!(input.content, "x你");
        },
    );
}

#[gpui::test]
fn selection_and_composition_end_invalidate_old_commands(cx: &mut gpui::TestAppContext) {
    with_input(
        cx,
        InputProps {
            initial_value: "hello".into(),
            ..Default::default()
        },
        |input, window, cx| {
            let before = input.revision;
            input.left(&Left, window, cx);
            assert!(
                input
                    .apply_command(
                        InputCommand::Replace {
                            value: "late".into(),
                            expected_revision: before
                        },
                        window,
                        cx
                    )
                    .is_err()
            );
            input.replace_and_mark_text_in_range(None, "é", Some(1..1), window, cx);
            let before = input.revision;
            input.unmark_text(window, cx);
            assert!(input.revision > before);
        },
    );
}

#[gpui::test]
fn grapheme_deletion_and_utf16_selection_are_consistent(cx: &mut gpui::TestAppContext) {
    with_input(
        cx,
        InputProps {
            initial_value: "a👩‍💻é".into(),
            ..Default::default()
        },
        |input, window, cx| {
            input.backspace(&Backspace, window, cx);
            assert_eq!(input.content, "a👩‍💻");
            input.backspace(&Backspace, window, cx);
            assert_eq!(input.content, "a");
            input.replace_text_in_range(None, "🙂z", window, cx);
            let revision = input.revision;
            assert!(
                input
                    .apply_command(
                        InputCommand::Select {
                            selection: Selection {
                                start: 2,
                                end: 2,
                                reversed: false
                            },
                            expected_revision: revision
                        },
                        window,
                        cx
                    )
                    .is_err()
            );
            assert_eq!(input.revision, revision);
            input
                .apply_command(
                    InputCommand::Select {
                        selection: Selection {
                            start: 1,
                            end: 3,
                            reversed: false,
                        },
                        expected_revision: revision,
                    },
                    window,
                    cx,
                )
                .unwrap();
            input.replace_text_in_range(None, "X", window, cx);
            assert_eq!(input.content, "aXz");
        },
    );
}

#[gpui::test]
fn readonly_blocks_native_editing_but_accepts_explicit_application_replacement(
    cx: &mut gpui::TestAppContext,
) {
    with_input(
        cx,
        InputProps {
            initial_value: "fixed".into(),
            read_only: true,
            ..Default::default()
        },
        |input, window, cx| {
            input.replace_text_in_range(None, "bad", window, cx);
            input.replace_and_mark_text_in_range(None, "bad", None, window, cx);
            input.backspace(&Backspace, window, cx);
            assert_eq!(input.content, "fixed");
            input
                .apply_command(
                    InputCommand::Replace {
                        value: "updated".into(),
                        expected_revision: input.revision,
                    },
                    window,
                    cx,
                )
                .unwrap();
            assert_eq!(input.content, "updated");
        },
    );
}

#[gpui::test]
fn caret_uses_the_gpui_clock(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(|_, cx| Input::new(InputProps::default(), cx));
    window
        .update(cx, |input, window, cx| {
            window.set_logical_active_for_tests(true);
            window.focus(&input.focus_handle, cx);
            assert!(input.caret_shown(window, cx));
        })
        .unwrap();
    cx.executor()
        .advance_clock(Duration::from_millis(CARET_BLINK_MS));
    window
        .update(cx, |input, window, cx| {
            assert!(!input.caret_shown(window, cx))
        })
        .unwrap();
}
