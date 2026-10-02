DROP TRIGGER IF EXISTS agent_mail_needs_transition;

DROP VIEW IF EXISTS agent_turn_comment_reply;

DROP VIEW IF EXISTS v_turn_cwd;

DROP VIEW IF EXISTS v_conversational_turn;

DROP VIEW IF EXISTS v_message;

DROP VIEW IF EXISTS v_favorite;

DROP VIEW IF EXISTS v_session_project;

DROP VIEW IF EXISTS v_usage_cost;

DROP VIEW IF EXISTS v_skill_cost_act;

DROP VIEW IF EXISTS v_skill_cost_window;

CREATE TABLE agent_trace_span_v40 (
  session_id INTEGER PRIMARY KEY,
  trace_id INTEGER NOT NULL,
  attach TEXT NOT NULL CHECK (attach IN ('backfill-spawned-edge','lane-create','lane-run','supervisor-conversation','native-tui-session','derived-session-relation')),
  attached_ts INTEGER NOT NULL
);

INSERT INTO agent_trace_span_v40 SELECT old.session_id, old.trace_id, (SELECT value FROM dict_attach WHERE id = old.attach_id), old.attached_ts FROM agent_trace_span old;

DROP TABLE agent_trace_span;

ALTER TABLE agent_trace_span_v40 RENAME TO agent_trace_span;

CREATE INDEX IF NOT EXISTS idx_span_trace ON agent_trace_span(trace_id);

CREATE TABLE agent_session_attr_v40 (
  session_id INTEGER NOT NULL,
  key TEXT NOT NULL CHECK (key IN ('effort','reset_ts','process_pid','process_start_secs','process_previous_session','mood')),
  value TEXT NOT NULL,
  set_ts INTEGER NOT NULL,
  PRIMARY KEY (session_id, key)
) WITHOUT ROWID;

INSERT INTO agent_session_attr_v40 SELECT old.session_id, (SELECT value FROM dict_attr_key WHERE id = old.key_id), old.value, old.set_ts FROM agent_session_attr old;

DROP TABLE agent_session_attr;

ALTER TABLE agent_session_attr_v40 RENAME TO agent_session_attr;

CREATE TABLE agent_edge_v40 (
  parent_session_id INTEGER NOT NULL,
  child_session_id INTEGER NOT NULL,
  edge_kind TEXT NOT NULL CHECK (edge_kind IN ('spawned','result','deliver-nextturn','hail','deliver-midturn','completed','completion-mailed','completion-delivered','cancel','retry','resume')),
  agent_type_id INTEGER,
  model_id INTEGER,
  first_ts INTEGER,
  last_ts INTEGER,
  n INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (parent_session_id, child_session_id, edge_kind)
) WITHOUT ROWID;

INSERT INTO agent_edge_v40 SELECT old.parent_session_id, old.child_session_id, (SELECT value FROM dict_edekind WHERE id = old.edge_kind_id), old.agent_type_id, old.model_id, old.first_ts, old.last_ts, old.n FROM agent_edge old;

DROP TABLE agent_edge;

ALTER TABLE agent_edge_v40 RENAME TO agent_edge;

CREATE INDEX IF NOT EXISTS idx_edge_child ON agent_edge(child_session_id, edge_kind);

CREATE TABLE agent_fetch_v40 (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  kind TEXT NOT NULL CHECK (kind IN ('fetch','search')),
  url_id INTEGER,
  domain_id INTEGER,
  query TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

INSERT INTO agent_fetch_v40 SELECT old.session_id, old.turn, old.ts, (SELECT value FROM dict_netkind WHERE id = old.kind_id), old.url_id, old.domain_id, old.query FROM agent_fetch old;

DROP TABLE agent_fetch;

ALTER TABLE agent_fetch_v40 RENAME TO agent_fetch;

CREATE INDEX IF NOT EXISTS idx_fetch_ts ON agent_fetch(ts);

CREATE TABLE agent_turn_v40 (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  role TEXT NOT NULL,
  said TEXT,
  cwd_id INTEGER,
  source_class TEXT NOT NULL DEFAULT 'unknown',
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

INSERT INTO agent_turn_v40 SELECT old.session_id, old.turn, old.ts, (SELECT value FROM dict_role WHERE id = old.role_id), old.said, old.cwd_id, old.source_class FROM agent_turn old;

DROP TABLE agent_turn;

ALTER TABLE agent_turn_v40 RENAME TO agent_turn;

CREATE INDEX IF NOT EXISTS idx_turn_session_ts ON agent_turn(session_id, ts);

CREATE TABLE agent_touch_v40 (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  path_id INTEGER NOT NULL,
  verb TEXT NOT NULL CHECK (verb IN ('read','Read','edit','Edit','write','Write','grep','Grep','glob','Glob','list','multiedit')),
  raw_verb TEXT,
  PRIMARY KEY (session_id, turn, path_id, verb)
) WITHOUT ROWID;

INSERT INTO agent_touch_v40 SELECT old.session_id, old.turn, old.ts, old.path_id, (SELECT value FROM dict_verb WHERE id = old.verb_id), (SELECT value FROM dict_verb WHERE id = old.raw_verb_id) FROM agent_touch old;

DROP TABLE agent_touch;

ALTER TABLE agent_touch_v40 RENAME TO agent_touch;

CREATE INDEX IF NOT EXISTS idx_touch_pathid ON agent_touch(path_id);

CREATE TABLE agent_session_relation_v40 (
  relation_id INTEGER PRIMARY KEY,
  relation_key TEXT NOT NULL UNIQUE,
  from_session INTEGER NOT NULL REFERENCES dict_session(id),
  to_session INTEGER NOT NULL REFERENCES dict_session(id),
  kind TEXT NOT NULL CHECK (kind IN ('continued-in','same-process','parent-child','same-pane','same-tui')),
  source TEXT NOT NULL CHECK (source IN ('legacy-agent-trace-span','trace-event','transcript-sync','trace-attach','live-status','transcript-session-metadata','claude-transcript')),
  observed_ts INTEGER NOT NULL,
  matched_identity_key TEXT
);

INSERT INTO agent_session_relation_v40 SELECT old.relation_id, old.relation_key, old.from_session, old.to_session, (SELECT value FROM dict_session_relation_kind WHERE id = old.kind_id), (SELECT value FROM dict_observation_source WHERE id = old.source_id), old.observed_ts, old.matched_identity_key FROM agent_session_relation old;

DROP TABLE agent_session_relation;

ALTER TABLE agent_session_relation_v40 RENAME TO agent_session_relation;

CREATE INDEX IF NOT EXISTS idx_session_relation_from ON agent_session_relation(from_session);

CREATE INDEX IF NOT EXISTS idx_session_relation_to ON agent_session_relation(to_session);

CREATE INDEX IF NOT EXISTS idx_session_relation_source_time
  ON agent_session_relation(source, observed_ts);

CREATE TABLE agent_session_observation_v40 (
  observation_id INTEGER PRIMARY KEY,
  observation_key TEXT NOT NULL UNIQUE,
  session_id INTEGER NOT NULL REFERENCES dict_session(id),
  observed_ts INTEGER NOT NULL,
  harness TEXT CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  cwd_id INTEGER REFERENCES dict_cwd(id),
  pid INTEGER,
  parent_pid INTEGER,
  pane_id INTEGER REFERENCES dict_pane(id),
  tui_session_id INTEGER REFERENCES dict_tui_session(id),
  source TEXT NOT NULL CHECK (source IN ('legacy-agent-trace-span','trace-event','transcript-sync','trace-attach','live-status','transcript-session-metadata','claude-transcript'))
);

INSERT INTO agent_session_observation_v40 SELECT old.observation_id, old.observation_key, old.session_id, old.observed_ts, (SELECT value FROM dict_harness WHERE id = old.harness_id), old.cwd_id, old.pid, old.parent_pid, old.pane_id, old.tui_session_id, (SELECT value FROM dict_observation_source WHERE id = old.source_id) FROM agent_session_observation old;

DROP TABLE agent_session_observation;

ALTER TABLE agent_session_observation_v40 RENAME TO agent_session_observation;

CREATE INDEX IF NOT EXISTS idx_session_observation_session_time
  ON agent_session_observation(session_id, observed_ts);

CREATE INDEX IF NOT EXISTS idx_session_observation_source_time
  ON agent_session_observation(source, observed_ts);

CREATE TABLE model_price_v40 (
  model_id INTEGER PRIMARY KEY,
  input_per_mtok REAL NOT NULL,
  output_per_mtok REAL NOT NULL,
  cache_write_5m_per_mtok REAL NOT NULL,
  cache_write_1h_per_mtok REAL NOT NULL,
  cache_read_per_mtok REAL NOT NULL,
  source TEXT NOT NULL,
  fetched_ts INTEGER NOT NULL
);

INSERT INTO model_price_v40 SELECT old.model_id, old.input_per_mtok, old.output_per_mtok, old.cache_write_5m_per_mtok, old.cache_write_1h_per_mtok, old.cache_read_per_mtok, (SELECT value FROM dict_price_source WHERE id = old.source_id), old.fetched_ts FROM model_price old;

DROP TABLE model_price;

ALTER TABLE model_price_v40 RENAME TO model_price;

CREATE TABLE agent_trace_event_v40 (
  event_id INTEGER PRIMARY KEY,
  event_key TEXT NOT NULL UNIQUE,
  lane_id INTEGER NOT NULL REFERENCES dict_session(id),
  trace_id INTEGER REFERENCES dict_trace(id),
  session_id INTEGER REFERENCES dict_session(id),
  from_lane_id INTEGER REFERENCES dict_session(id),
  to_lane_id INTEGER REFERENCES dict_session(id),
  kind TEXT NOT NULL CHECK (kind IN ('supervisor-start','channel-open','turn-start','error','supervisor-exit','turn-finish','delivery','idle-shutdown','cli-invocation','harness-quiet','harness-active','session-boundary','resource-sample','resource-interrupt','resource-pause','resource-resume','parent-death','stale')),
  started_ts INTEGER,
  finished_ts INTEGER,
  delivery_state TEXT CHECK (delivery_state IN ('midturn','nextturn','started','ok','version','help','parse-error','error')),
  classification TEXT CHECK (classification IN ('starting','opened','started','failed','completed','retryable','delivered','queued','retired','quiet','active','same-process','over-limit','within-limit','accepted','paused','resumed','stale')),
  detail TEXT NOT NULL DEFAULT '',
  created_ts INTEGER NOT NULL
);

INSERT INTO agent_trace_event_v40 SELECT old.event_id, old.event_key, old.lane_id, old.trace_id, old.session_id, old.from_lane_id, old.to_lane_id, (SELECT value FROM dict_trace_kind WHERE id = old.kind_id), old.started_ts, old.finished_ts, (SELECT value FROM dict_trace_delivery WHERE id = old.delivery_state_id), (SELECT value FROM dict_trace_classification WHERE id = old.classification_id), old.detail, old.created_ts FROM agent_trace_event old;

DROP TABLE agent_trace_event;

ALTER TABLE agent_trace_event_v40 RENAME TO agent_trace_event;

CREATE INDEX IF NOT EXISTS idx_trace_event_lane_time
  ON agent_trace_event(lane_id, created_ts, event_id);

CREATE INDEX IF NOT EXISTS idx_trace_event_trace_time
  ON agent_trace_event(trace_id, created_ts, event_id);

CREATE TABLE agent_live_v40 (
  session_id INTEGER PRIMARY KEY,
  pid INTEGER,
  tmux_pane_id INTEGER,
  status TEXT CHECK (status IN ('idle','live','detached','dead','closed')),
  door_kind TEXT,
  door_addr TEXT,
  last_seen_ts INTEGER,
  pane_alive INTEGER,
  pid_alive INTEGER,
  tmux_session TEXT
);

INSERT INTO agent_live_v40 SELECT old.session_id, old.pid, old.tmux_pane_id, (SELECT value FROM dict_status WHERE id = old.status_id), old.door_kind, old.door_addr, old.last_seen_ts, old.pane_alive, old.pid_alive, old.tmux_session FROM agent_live old;

DROP TABLE agent_live;

ALTER TABLE agent_live_v40 RENAME TO agent_live;

CREATE TABLE agent_live_span_v40 (
  session_id INTEGER NOT NULL,
  from_ts INTEGER NOT NULL,
  to_ts INTEGER,
  status TEXT NOT NULL CHECK (status IN ('idle','live','detached','dead','closed')),
  pid INTEGER,
  tmux_pane_id INTEGER,
  PRIMARY KEY (session_id, from_ts)
) WITHOUT ROWID;

INSERT INTO agent_live_span_v40 SELECT old.session_id, old.from_ts, old.to_ts, (SELECT value FROM dict_status WHERE id = old.status_id), old.pid, old.tmux_pane_id FROM agent_live_span old;

DROP TABLE agent_live_span;

ALTER TABLE agent_live_span_v40 RENAME TO agent_live_span;

CREATE INDEX IF NOT EXISTS idx_live_span_open ON agent_live_span(session_id, to_ts);

CREATE TABLE agent_session_v40 (
  session_id INTEGER PRIMARY KEY,
  harness TEXT NOT NULL CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  nickname TEXT,
  cwd_id INTEGER,
  branch_id INTEGER,
  started_ts INTEGER
);

INSERT INTO agent_session_v40 SELECT old.session_id, (SELECT value FROM dict_harness WHERE id = old.harness_id), old.nickname, old.cwd_id, old.branch_id, old.started_ts FROM agent_session old;

DROP TABLE agent_session;

ALTER TABLE agent_session_v40 RENAME TO agent_session;

CREATE INDEX IF NOT EXISTS idx_session_cwd ON agent_session(cwd_id);

CREATE TABLE agent_lane_v40 (
  spawn_id INTEGER PRIMARY KEY,
  lane_id INTEGER NOT NULL,
  trace_id INTEGER,
  harness TEXT CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  branch_id INTEGER,
  cwd_id INTEGER,
  model_id INTEGER,
  parent_lane_id INTEGER,
  goal TEXT,
  brief_path_id INTEGER,
  brief_markdown_id INTEGER,
  spawned_ts INTEGER NOT NULL
);

INSERT INTO agent_lane_v40 SELECT old.spawn_id, old.lane_id, old.trace_id, (SELECT value FROM dict_harness WHERE id = old.harness_id), old.branch_id, old.cwd_id, old.model_id, old.parent_lane_id, old.goal, old.brief_path_id, old.brief_markdown_id, old.spawned_ts FROM agent_lane old;

DROP TABLE agent_lane;

ALTER TABLE agent_lane_v40 RENAME TO agent_lane;

CREATE INDEX IF NOT EXISTS idx_lane_trace ON agent_lane(trace_id);

CREATE INDEX IF NOT EXISTS idx_lane_lane ON agent_lane(lane_id, spawned_ts);

CREATE TABLE agent_delivery_v40 (
  message_id TEXT NOT NULL,
  route TEXT NOT NULL,
  harness TEXT CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  outcome TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (message_id, route)
) WITHOUT ROWID;

INSERT INTO agent_delivery_v40 SELECT old.message_id, old.route, (SELECT value FROM dict_harness WHERE id = old.harness_id), old.outcome, old.detail, old.at_ms FROM agent_delivery old;

DROP TABLE agent_delivery;

ALTER TABLE agent_delivery_v40 RENAME TO agent_delivery;

CREATE TABLE agent_delivery_transition_v40 (
  message_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  route TEXT NOT NULL,
  harness TEXT CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  outcome TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  error_code TEXT,
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (message_id, sequence)
) WITHOUT ROWID;

INSERT INTO agent_delivery_transition_v40 SELECT old.message_id, old.sequence, old.route, (SELECT value FROM dict_harness WHERE id = old.harness_id), old.outcome, old.detail, old.error_code, old.at_ms FROM agent_delivery_transition old;

DROP TABLE agent_delivery_transition;

ALTER TABLE agent_delivery_transition_v40 RENAME TO agent_delivery_transition;

CREATE INDEX IF NOT EXISTS idx_delivery_transition_route
  ON agent_delivery_transition(message_id, route);

CREATE TABLE sync_root_stamp_v40 (
  harness TEXT NOT NULL,
  root_path_id INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL,
  PRIMARY KEY (harness, root_path_id)
) WITHOUT ROWID;

INSERT INTO sync_root_stamp_v40 SELECT (SELECT value FROM dict_harness WHERE id = old.harness_id), old.root_path_id, old.mtime_ms FROM sync_root_stamp old;

DROP TABLE sync_root_stamp;

ALTER TABLE sync_root_stamp_v40 RENAME TO sync_root_stamp;

DROP TABLE dict_attr_key;

DROP TABLE dict_harness;

DROP TABLE dict_observation_source;

DROP TABLE dict_role;

DROP TABLE dict_session_relation_kind;

DROP TABLE dict_status;

DROP TABLE dict_price_source;

DROP TABLE dict_attach;

DROP TABLE dict_edekind;

DROP TABLE dict_netkind;

DROP TABLE dict_trace_classification;

DROP TABLE dict_trace_delivery;

DROP TABLE dict_trace_kind;

DROP TABLE dict_verb;

CREATE VIEW agent_turn_comment_reply AS SELECT c.comment_id,
       t.session_id,
       t.turn AS target_turn,
       (SELECT MIN(a.turn)
          FROM agent_turn a

         WHERE a.session_id = t.session_id
           AND a.turn > t.turn
           AND a.role = 'assistant'
           AND (a.ts IS NULL OR a.ts >= c.sent_ts)) AS reply_turn
  FROM agent_turn_comment c
  JOIN agent_turn_comment_target t ON t.comment_id = c.comment_id
 WHERE c.sent_ts IS NOT NULL;

CREATE VIEW v_turn_cwd AS SELECT t.session_id,
       t.turn,
       cwd.value AS cwd
  FROM agent_turn t
  JOIN agent_session s ON s.session_id = t.session_id
  LEFT JOIN dict_cwd cwd ON cwd.id = COALESCE(t.cwd_id, s.cwd_id);

CREATE VIEW v_conversational_turn AS SELECT turn.session_id,
       session.value AS session,
       turn.turn,
       turn.role AS role,
       turn.said AS body,
       turn.ts,
       cwd.value AS cwd,
       turn.source_class
  FROM agent_turn turn
  JOIN dict_session session ON session.id = turn.session_id

  LEFT JOIN agent_session session_row ON session_row.session_id = turn.session_id
  LEFT JOIN dict_cwd cwd ON cwd.id = COALESCE(turn.cwd_id, session_row.cwd_id)
 WHERE turn.role IN ('user', 'assistant')
   AND NOT (turn.role = 'user' AND turn.source_class = 'harness');

CREATE VIEW v_message AS SELECT current.session,
       current.turn,
       (SELECT COUNT(*)
          FROM v_conversational_turn counted
         WHERE counted.session_id = current.session_id
           AND counted.turn <= current.turn) AS n,
       current.role,
       current.body,
       current.ts,
       current.cwd,
       current.source_class,
       CASE WHEN EXISTS (
         SELECT 1 FROM v_conversational_turn previous
          WHERE previous.session_id = current.session_id
            AND previous.role = 'user' AND previous.turn < current.turn
       ) THEN (
         SELECT COUNT(*)
           FROM v_conversational_turn counted
          WHERE counted.session_id = current.session_id
            AND counted.turn <= (
              SELECT MAX(previous.turn)
                FROM v_conversational_turn previous
               WHERE previous.session_id = current.session_id
                 AND previous.role = 'user'
                 AND previous.turn < current.turn
            )
       ) END AS prev_user_n,
       CASE WHEN EXISTS (
         SELECT 1 FROM v_conversational_turn previous
          WHERE previous.session_id = current.session_id
            AND previous.role = 'assistant' AND previous.turn < current.turn
       ) THEN (
         SELECT COUNT(*)
           FROM v_conversational_turn counted
          WHERE counted.session_id = current.session_id
            AND counted.turn <= (
              SELECT MAX(previous.turn)
                FROM v_conversational_turn previous
               WHERE previous.session_id = current.session_id
                 AND previous.role = 'assistant'
                 AND previous.turn < current.turn
            )
       ) END AS prev_assistant_n
  FROM v_conversational_turn current;

CREATE VIEW v_favorite AS SELECT favorite.favorite_id AS id, favorite.note, markdown.body,
       favorite.source_text AS source, favorite.source_session AS session,
       CASE WHEN favorite.source_turn_end IS NULL THEN favorite.source_turn END AS turn,
       message.n, message.source_class
  FROM agent_favorite favorite
  JOIN markdown_cache markdown ON markdown.markdown_id = favorite.markdown_id
  LEFT JOIN v_message message
    ON message.session = favorite.source_session
   AND message.turn = favorite.source_turn
   AND message.role = 'assistant'
   AND favorite.source_turn_end IS NULL;

CREATE VIEW v_session_project AS WITH projects AS (
  SELECT DISTINCT cwd AS project
    FROM v_turn_cwd
   WHERE cwd IS NOT NULL AND cwd <> ''
  UNION
  SELECT DISTINCT cwd.value AS project
    FROM agent_session session
    JOIN dict_cwd cwd ON cwd.id = session.cwd_id
   WHERE cwd.value <> ''
), session_paths AS (
  SELECT session_id, cwd AS path
    FROM v_turn_cwd
   WHERE cwd IS NOT NULL AND cwd <> ''
  UNION
  SELECT session.session_id, cwd.value AS path
    FROM agent_session session
    JOIN dict_cwd cwd ON cwd.id = session.cwd_id
   WHERE cwd.value <> ''
), touch_paths AS (
  SELECT touch.session_id,
         CASE WHEN substr(path.value, 1, 1) = '/' THEN path.value
              WHEN turn_cwd.cwd IS NOT NULL
                THEN rtrim(turn_cwd.cwd, '/') || '/' || path.value
              ELSE path.value
         END AS path
    FROM agent_touch touch
    JOIN dict_path path ON path.id = touch.path_id
    LEFT JOIN v_turn_cwd turn_cwd
      ON turn_cwd.session_id = touch.session_id AND turn_cwd.turn = touch.turn
), memberships AS (
  SELECT session.session_id, projects.project, 'cwd' AS evidence
    FROM session_paths session
    JOIN projects ON session.path = projects.project
      OR (substr(session.path, 1, length(projects.project)) = projects.project
          AND (substr(projects.project, -1, 1) = '/'
               OR substr(session.path, length(projects.project) + 1, 1) = '/'))
  UNION ALL
  SELECT touch.session_id, projects.project, 'touch' AS evidence
    FROM touch_paths touch
    JOIN projects ON touch.path = projects.project
      OR (substr(touch.path, 1, length(projects.project)) = projects.project
          AND (substr(projects.project, -1, 1) = '/'
               OR substr(touch.path, length(projects.project) + 1, 1) = '/'))
)
SELECT session.value AS session,
       memberships.project,
       CASE WHEN MAX(memberships.evidence = 'cwd') AND MAX(memberships.evidence = 'touch')
              THEN 'cwd+touch'
            WHEN MAX(memberships.evidence = 'cwd') THEN 'cwd'
            ELSE 'touch'
       END AS evidence
  FROM memberships
  JOIN dict_session session ON session.id = memberships.session_id
 GROUP BY session.value, memberships.project;

CREATE VIEW v_usage_cost AS SELECT usage.session_id,
       usage.turn,
       usage.ts,
       dict_session.value AS session,
       agent_session.harness AS harness,
       dict_model.value AS model,
       usage.input_tokens,
       usage.output_tokens,
       usage.cache_create_5m_tokens,
       usage.cache_create_1h_tokens,
       usage.cache_read_tokens,
       usage.is_sidechain,
       COALESCE(usage.cost_usd_recorded,
                (usage.input_tokens * price.input_per_mtok
                 + usage.output_tokens * price.output_per_mtok
                 + usage.cache_create_5m_tokens * price.cache_write_5m_per_mtok
                 + usage.cache_create_1h_tokens * price.cache_write_1h_per_mtok
                 + usage.cache_read_tokens * price.cache_read_per_mtok) / 1e6) AS cost_usd
FROM agent_usage AS usage
JOIN agent_session ON agent_session.session_id = usage.session_id
JOIN dict_session ON dict_session.id = agent_session.session_id

JOIN dict_model ON dict_model.id = usage.model_id
LEFT JOIN model_price AS price ON price.model_id = usage.model_id;

CREATE VIEW v_skill_cost_act AS SELECT dict_skill.value AS skill,
       v_usage_cost.session AS session,
       v_usage_cost.harness AS harness,
       v_usage_cost.turn AS act_turn,
       v_usage_cost.ts AS ts,
       v_usage_cost.model AS model,
       v_usage_cost.input_tokens,
       v_usage_cost.output_tokens,
       v_usage_cost.cache_read_tokens,
       v_usage_cost.cost_usd
FROM agent_skill
JOIN dict_skill ON dict_skill.id = agent_skill.skill_id
JOIN v_usage_cost ON v_usage_cost.session_id = agent_skill.session_id
                 AND v_usage_cost.turn = agent_skill.turn;

CREATE VIEW v_skill_cost_window AS WITH act AS (
  SELECT agent_skill.session_id AS session_id,
         agent_skill.skill_id AS skill_id,
         agent_skill.turn AS act_turn,
         (SELECT MIN(agent_turn.turn) FROM agent_turn
          WHERE agent_turn.session_id = agent_skill.session_id
            AND agent_turn.role = 'user'
            AND agent_turn.turn > agent_skill.turn) AS next_user_turn
  FROM agent_skill
)
SELECT dict_skill.value AS skill,
       dict_session.value AS session,
       act.act_turn,
       act.next_user_turn,
       COUNT(v_usage_cost.turn) AS usage_rows,
       CAST(TOTAL(v_usage_cost.is_sidechain) AS INTEGER) AS sidechain_rows,
       CAST(TOTAL(v_usage_cost.input_tokens) AS INTEGER) AS input_tokens,
       CAST(TOTAL(v_usage_cost.output_tokens) AS INTEGER) AS output_tokens,
       TOTAL(v_usage_cost.cost_usd) AS cost_usd
FROM act
JOIN dict_skill ON dict_skill.id = act.skill_id
JOIN dict_session ON dict_session.id = act.session_id
LEFT JOIN v_usage_cost ON v_usage_cost.session_id = act.session_id
                      AND v_usage_cost.turn >= act.act_turn
                      AND v_usage_cost.turn < COALESCE(act.next_user_turn, 1000000000)
GROUP BY act.session_id, act.skill_id;

CREATE TRIGGER IF NOT EXISTS agent_mail_needs_transition
BEFORE INSERT ON agent_mail
WHEN NOT EXISTS (
  SELECT 1 FROM agent_delivery_transition WHERE message_id = NEW.message_id
)
BEGIN
  SELECT RAISE(ABORT, 'agent_mail row without a delivery transition');
END;
