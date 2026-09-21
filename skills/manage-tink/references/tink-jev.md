# Pick a skill with Jev Choice

Use this protocol when the user asks which skill fits a task and no mutation is authorized yet. For exact list/read command forms see `references/commands.md`.

## 1. Gather candidates

Collect the user's task words, then list project skills with `tink skill list` and read descriptions with `tink skill read`. Project first; fall back to `--library` when the project bin is empty or too thin. Use descriptions only — do not paste full skill bodies into the pick request.

## 2. Call Jev Choice

Requires network access and `TYPESAFE_API_KEY` in the environment. If either is missing, stop and refuse cleanly (see Refusal).

Request:

- `POST https://api.typesafe.ai/v1/systemone`
- Header: `Authorization: Bearer $TYPESAFE_API_KEY`
- Body:
  ```json
  {
    "state": "<task words + candidate descriptions>",
    "model": "jev-latest",
    "questions": {
      "pick": {
        "type": "choice",
        "instructions": "Which skill best fits the task?",
        "criteria": {"<skill-name>": "<description>", "...": "..."}
      }
    }
  }
  ```

Max 255 options per Choice. Answers come back keyed by question id (`pick`) with `{choice, confidence, probabilities}` where confidence is 0–1.

## 3. Report

Report the top pick (the returned `choice`) plus its reason. Then apply the unsure rule:

- Unsure with runner-up when `confidence < 0.6` OR (`top1 − top2 < 0.1`), where top1/top2 are the two highest values in `probabilities`. Runner-up is the second-highest entry in `probabilities`.
- Empty bin: report empty, never guess.

## Refusal

Offline or missing `TYPESAFE_API_KEY`: refuse cleanly ("no 8-ball in caves"), never guess a skill.

## Privacy

Send ONLY task words + skill descriptions to Jev. The key is never printed, saved, or logged.
