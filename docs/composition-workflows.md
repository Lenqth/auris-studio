# Experimental composition workflows

Set `AURIS_COMPOSITION_WORKFLOWS=1` before starting Auris to test the compact
common prompt and task-specific workflows. The complete tool catalog stays the
same, so prompt experiments do not silently compare different tools. The default
continues to use the existing prompt until listening evidence supports a change.
Use `AURIS_COMPOSITION_CANDIDATES=1` independently to advertise candidate tools;
keep that flag equal in both prompt conditions. The evaluation runner enables it
in both conditions and uses 64k for local-model experiments.

`production_record` accepts `select_workflow` with `workflow` (`composition`,
`accompaniment`, `vocals`, `local_edit`, `delivery`) and `writing_order`
(`harmony_first`, `melody_first`, `rhythm_first`). Selection is conversation
metadata, subject to the same write permission as other record updates. It
survives context compaction and conversation restart. The response, and later
project inspections, provide only the selected task's instructions in modular
mode. The agent may explicitly change the selection when the task changes.

The shared contract retains exact IDs, tick units, revision checks, source
preservation, limits on exploration, and the distinction between measurements,
critic reports and human listening. Workflow instructions are model guidance;
the canonical session commands enforce validation and permissions independently.

For a controlled comparison, use the same frozen executable, model, task cohort
and tool catalog with `composition_quality.py run --prompt-profile legacy` and
`--prompt-profile modular`. Record failures and elapsed time alongside backend
token counters. Last-reported input context is not cumulative billable input;
missing usage must not be presented as zero cost. Use blinded human preference
and request adherence before adopting the modular profile as the default.
