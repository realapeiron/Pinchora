//! User-bound, expiring multi-page ticket forms.
use super::*;

/// The form a panel asks before a ticket opens, as `tf:<panel>:<topic>`.
pub(crate) async fn on_form(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let picked = parts
        .next()
        .and_then(|p| p.parse::<i64>().ok())
        .zip(parts.next().and_then(|t| t.parse::<usize>().ok()));
    let step = parts.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
    let source = parse_source(&mut parts);
    let Some((panel, topic)) = picked else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let Some((panel, topic)) = usable(cx, panel, topic).await? else {
        return Ok(());
    };
    let ops = cx.app.db.ticket_panel_operations(panel.id).await?;
    if let Some(blocked) = eligibility(cx, &ops).await? {
        return tell(cx, blocked).await;
    }
    let Some(start) = step.checked_mul(5).filter(|start| *start < panel.questions.len()) else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let Some(answers) = page_answers(&panel.questions, start, fields) else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let guild = cx
        .guild_id()
        .ok_or_else(|| anyhow::anyhow!("ticket form outside guild"))?;
    let user = cx.author()?.id.get();
    let index = panel.topics.iter().position(|t| t == &topic).unwrap_or_default();
    let Some((saved, expected)) = cx.app.db.ticket_form_draft(guild, user, panel.id, index as i32).await? else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    if expected != step as i32 {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    }
    let mut all = serde_json::from_value::<Vec<Answer>>(saved)?;
    all.extend(answers);
    if !cx
        .app
        .db
        .advance_ticket_form_draft(
            guild,
            user,
            panel.id,
            index as i32,
            &serde_json::to_value(&all)?,
            step as i32,
        )
        .await?
    {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    }
    if start + 5 < panel.questions.len() {
        let next = step + 1;
        return cx
            .reply(private(
                t!(
                    cx.lang(),
                    "tickets.v2.next-step",
                    current = next,
                    total = panel.questions.len().div_ceil(5)
                ),
                vec![row(vec![button(
                    source.map_or_else(
                        || format!("tk:f:{}:{index}", panel.id),
                        |(channel, message)| format!("tk:f:{}:{index}:{channel}:{message}", panel.id),
                    ),
                    &t!(cx.lang(), "tickets.v2.continue"),
                    None,
                    ButtonStyle::Primary,
                    false,
                )])],
            ))
            .await;
    }
    open(cx, &panel, topic, with_source(cx, all, source)).await
}

fn page_answers(questions: &[Question], start: usize, fields: &[(String, String)]) -> Option<Vec<Answer>> {
    questions
        .iter()
        .skip(start)
        .take(5)
        .enumerate()
        .map(|(n, question)| {
            let matches: Vec<_> = fields.iter().filter(|(id, _)| *id == question_id(n)).collect();
            if matches.len() > 1 {
                return None;
            }
            let value = matches.first().map(|(_, value)| value.trim()).unwrap_or("");
            if (question.required && value.is_empty())
                || value.chars().count() > usize::from(ANSWER_MAX)
                || (!value.is_empty() && !question.choices.is_empty() && !question.choices.iter().any(|c| c == value))
            {
                return None;
            }
            Some(Answer {
                question: question.label.clone(),
                answer: value.to_owned(),
            })
        })
        .collect()
}

#[allow(deprecated)] // TextInput::label: the Label around it carries the label now
pub(super) async fn ask(
    cx: &Cx,
    panel: &TicketPanel,
    topic: usize,
    questions: &[Question],
    step: usize,
    source: Option<(u64, u64)>,
) -> anyhow::Result<()> {
    let fields = questions
        .iter()
        .skip(step * 5)
        .take(5)
        .enumerate()
        .map(|(n, question)| {
            let input = if question.upload {
                Component::FileUpload(FileUpload {
                    custom_id: question_id(n),
                    id: None,
                    max_values: Some(1),
                    min_values: Some(u8::from(question.required)),
                    required: Some(question.required),
                    file_types: None,
                })
            } else if !question.choices.is_empty() {
                Component::SelectMenu(SelectMenu {
                    channel_types: None,
                    custom_id: question_id(n),
                    default_values: None,
                    disabled: false,
                    id: None,
                    kind: SelectMenuType::Text,
                    max_values: Some(1),
                    min_values: Some(u8::from(question.required)),
                    options: Some(
                        question
                            .choices
                            .iter()
                            .map(|choice| SelectMenuOption {
                                default: false,
                                description: None,
                                emoji: None,
                                label: choice.clone(),
                                value: choice.clone(),
                            })
                            .collect(),
                    ),
                    placeholder: Some(question.placeholder.clone()).filter(|s| !s.is_empty()),
                    required: Some(question.required),
                })
            } else {
                Component::TextInput(TextInput {
                    custom_id: question_id(n),
                    id: None,
                    label: None,
                    max_length: Some(ANSWER_MAX),
                    min_length: question.required.then_some(1),
                    placeholder: Some(question.placeholder.clone()).filter(|p| !p.trim().is_empty()),
                    required: Some(question.required),
                    style: if question.long {
                        TextInputStyle::Paragraph
                    } else {
                        TextInputStyle::Short
                    },
                    value: None,
                })
            };
            Component::Label(Label {
                component: Box::new(input),
                description: None,
                id: None,
                label: question
                    .label
                    .chars()
                    .take(pinchora_core::tickets::QUESTION_MAX)
                    .collect(),
            })
        })
        .collect();
    let title: String = single_line(&panel.title).chars().take(45).collect();
    let id = source.map_or_else(
        || format!("tf:{}:{topic}:{step}", panel.id),
        |(channel, message)| format!("tf:{}:{topic}:{step}:{channel}:{message}", panel.id),
    );
    cx.modal(id, &title, fields).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_choices_and_answer_caps_are_checked_server_side() {
        let q = Question {
            label: "Choice".into(),
            placeholder: String::new(),
            required: true,
            long: false,
            choices: vec!["Yes".into()],
            upload: false,
        };
        assert!(page_answers(std::slice::from_ref(&q), 0, &[]).is_none());
        for answer in ["No".to_owned(), "x".repeat(1001)] {
            assert!(page_answers(std::slice::from_ref(&q), 0, &[("q0".into(), answer)]).is_none());
        }
        let answer = ("q0".into(), "Yes".into());
        assert!(page_answers(std::slice::from_ref(&q), 0, &[answer.clone(), answer.clone()]).is_none());
        assert_eq!(page_answers(&[q], 0, &[answer]).unwrap()[0].answer, "Yes");
    }
}
