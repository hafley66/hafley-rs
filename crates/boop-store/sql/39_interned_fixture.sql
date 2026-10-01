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

CREATE TABLE dict_attr_key (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_harness (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_observation_source (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_role (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_session_relation_kind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_status (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_price_source (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_attach (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_edekind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_netkind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_trace_classification (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_trace_delivery (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_trace_kind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

CREATE TABLE dict_verb (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

INSERT OR IGNORE INTO dict_attach(value) SELECT DISTINCT attach FROM agent_trace_span WHERE attach IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_attr_key(value) SELECT DISTINCT key FROM agent_session_attr WHERE key IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_edekind(value) SELECT DISTINCT edge_kind FROM agent_edge WHERE edge_kind IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_netkind(value) SELECT DISTINCT kind FROM agent_fetch WHERE kind IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_role(value) SELECT DISTINCT role FROM agent_turn WHERE role IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_verb(value) SELECT DISTINCT verb FROM agent_touch WHERE verb IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_verb(value) SELECT DISTINCT raw_verb FROM agent_touch WHERE raw_verb IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_session_relation_kind(value) SELECT DISTINCT kind FROM agent_session_relation WHERE kind IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_observation_source(value) SELECT DISTINCT source FROM agent_session_relation WHERE source IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_harness(value) SELECT DISTINCT harness FROM agent_session_observation WHERE harness IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_observation_source(value) SELECT DISTINCT source FROM agent_session_observation WHERE source IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_price_source(value) SELECT DISTINCT source FROM model_price WHERE source IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_trace_kind(value) SELECT DISTINCT kind FROM agent_trace_event WHERE kind IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_trace_delivery(value) SELECT DISTINCT delivery_state FROM agent_trace_event WHERE delivery_state IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_trace_classification(value) SELECT DISTINCT classification FROM agent_trace_event WHERE classification IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_status(value) SELECT DISTINCT status FROM agent_live WHERE status IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_status(value) SELECT DISTINCT status FROM agent_live_span WHERE status IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_harness(value) SELECT DISTINCT harness FROM agent_session WHERE harness IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_harness(value) SELECT DISTINCT harness FROM agent_lane WHERE harness IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_harness(value) SELECT DISTINCT harness FROM agent_delivery WHERE harness IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_harness(value) SELECT DISTINCT harness FROM agent_delivery_transition WHERE harness IS NOT NULL ORDER BY 1;

INSERT OR IGNORE INTO dict_harness(value) SELECT DISTINCT harness FROM sync_root_stamp WHERE harness IS NOT NULL ORDER BY 1;

CREATE TABLE agent_trace_span_v39 (
  session_id INTEGER PRIMARY KEY,
  trace_id INTEGER NOT NULL,
  attach_id INTEGER NOT NULL,
  attached_ts INTEGER NOT NULL
);

INSERT INTO agent_trace_span_v39 SELECT old.session_id, old.trace_id, (SELECT id FROM dict_attach WHERE value=old.attach), old.attached_ts FROM agent_trace_span old;

DROP TABLE agent_trace_span;

ALTER TABLE agent_trace_span_v39 RENAME TO agent_trace_span;

CREATE INDEX IF NOT EXISTS idx_span_trace ON agent_trace_span(trace_id);

CREATE TABLE agent_session_attr_v39 (
  session_id INTEGER NOT NULL,
  key_id INTEGER NOT NULL,
  value TEXT NOT NULL,
  set_ts INTEGER NOT NULL,
  PRIMARY KEY (session_id, key_id)
) WITHOUT ROWID;

INSERT INTO agent_session_attr_v39 SELECT old.session_id, (SELECT id FROM dict_attr_key WHERE value=old.key), old.value, old.set_ts FROM agent_session_attr old;

DROP TABLE agent_session_attr;

ALTER TABLE agent_session_attr_v39 RENAME TO agent_session_attr;

CREATE TABLE agent_edge_v39 (
  parent_session_id INTEGER NOT NULL,
  child_session_id INTEGER NOT NULL,
  edge_kind_id INTEGER NOT NULL,
  agent_type_id INTEGER,
  model_id INTEGER,
  first_ts INTEGER,
  last_ts INTEGER,
  n INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (parent_session_id, child_session_id, edge_kind_id)
) WITHOUT ROWID;

INSERT INTO agent_edge_v39 SELECT old.parent_session_id, old.child_session_id, (SELECT id FROM dict_edekind WHERE value=old.edge_kind), old.agent_type_id, old.model_id, old.first_ts, old.last_ts, old.n FROM agent_edge old;

DROP TABLE agent_edge;

ALTER TABLE agent_edge_v39 RENAME TO agent_edge;

CREATE INDEX IF NOT EXISTS idx_edge_child ON agent_edge(child_session_id, edge_kind_id);

CREATE TABLE agent_fetch_v39 (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  kind_id INTEGER NOT NULL,
  url_id INTEGER,
  domain_id INTEGER,
  query TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

INSERT INTO agent_fetch_v39 SELECT old.session_id, old.turn, old.ts, (SELECT id FROM dict_netkind WHERE value=old.kind), old.url_id, old.domain_id, old.query FROM agent_fetch old;

DROP TABLE agent_fetch;

ALTER TABLE agent_fetch_v39 RENAME TO agent_fetch;

CREATE INDEX IF NOT EXISTS idx_fetch_ts ON agent_fetch(ts);

CREATE TABLE agent_turn_v39 (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  role_id INTEGER NOT NULL,
  said TEXT,
  cwd_id INTEGER,
  source_class TEXT NOT NULL DEFAULT 'unknown',
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

INSERT INTO agent_turn_v39 SELECT old.session_id, old.turn, old.ts, (SELECT id FROM dict_role WHERE value=old.role), old.said, old.cwd_id, old.source_class FROM agent_turn old;

DROP TABLE agent_turn;

ALTER TABLE agent_turn_v39 RENAME TO agent_turn;

CREATE INDEX IF NOT EXISTS idx_turn_session_ts ON agent_turn(session_id, ts);

CREATE TABLE agent_touch_v39 (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  path_id INTEGER NOT NULL,
  verb_id INTEGER NOT NULL,
  raw_verb_id INTEGER,
  PRIMARY KEY (session_id, turn, path_id, verb_id)
) WITHOUT ROWID;

INSERT INTO agent_touch_v39 SELECT old.session_id, old.turn, old.ts, old.path_id, (SELECT id FROM dict_verb WHERE value=old.verb), (SELECT id FROM dict_verb WHERE value=old.raw_verb) FROM agent_touch old;

DROP TABLE agent_touch;

ALTER TABLE agent_touch_v39 RENAME TO agent_touch;

CREATE INDEX IF NOT EXISTS idx_touch_pathid ON agent_touch(path_id);

CREATE TABLE agent_session_relation_v39 (
  relation_id INTEGER PRIMARY KEY,
  relation_key TEXT NOT NULL UNIQUE,
  from_session INTEGER NOT NULL REFERENCES dict_session(id),
  to_session INTEGER NOT NULL REFERENCES dict_session(id),
  kind_id INTEGER NOT NULL REFERENCES dict_session_relation_kind(id),
  source_id INTEGER NOT NULL REFERENCES dict_observation_source(id),
  observed_ts INTEGER NOT NULL,
  matched_identity_key TEXT
);

INSERT INTO agent_session_relation_v39 SELECT old.relation_id, old.relation_key, old.from_session, old.to_session, (SELECT id FROM dict_session_relation_kind WHERE value=old.kind), (SELECT id FROM dict_observation_source WHERE value=old.source), old.observed_ts, old.matched_identity_key FROM agent_session_relation old;

DROP TABLE agent_session_relation;

ALTER TABLE agent_session_relation_v39 RENAME TO agent_session_relation;

CREATE INDEX IF NOT EXISTS idx_session_relation_from ON agent_session_relation(from_session);

CREATE INDEX IF NOT EXISTS idx_session_relation_to ON agent_session_relation(to_session);

CREATE INDEX IF NOT EXISTS idx_session_relation_source_time
  ON agent_session_relation(source_id, observed_ts);

CREATE TABLE agent_session_observation_v39 (
  observation_id INTEGER PRIMARY KEY,
  observation_key TEXT NOT NULL UNIQUE,
  session_id INTEGER NOT NULL REFERENCES dict_session(id),
  observed_ts INTEGER NOT NULL,
  harness_id INTEGER REFERENCES dict_harness(id),
  cwd_id INTEGER REFERENCES dict_cwd(id),
  pid INTEGER,
  parent_pid INTEGER,
  pane_id INTEGER REFERENCES dict_pane(id),
  tui_session_id INTEGER REFERENCES dict_tui_session(id),
  source_id INTEGER NOT NULL REFERENCES dict_observation_source(id)
);

INSERT INTO agent_session_observation_v39 SELECT old.observation_id, old.observation_key, old.session_id, old.observed_ts, (SELECT id FROM dict_harness WHERE value=old.harness), old.cwd_id, old.pid, old.parent_pid, old.pane_id, old.tui_session_id, (SELECT id FROM dict_observation_source WHERE value=old.source) FROM agent_session_observation old;

DROP TABLE agent_session_observation;

ALTER TABLE agent_session_observation_v39 RENAME TO agent_session_observation;

CREATE INDEX IF NOT EXISTS idx_session_observation_session_time
  ON agent_session_observation(session_id, observed_ts);

CREATE INDEX IF NOT EXISTS idx_session_observation_source_time
  ON agent_session_observation(source_id, observed_ts);

CREATE TABLE model_price_v39 (
  model_id INTEGER PRIMARY KEY,
  input_per_mtok REAL NOT NULL,
  output_per_mtok REAL NOT NULL,
  cache_write_5m_per_mtok REAL NOT NULL,
  cache_write_1h_per_mtok REAL NOT NULL,
  cache_read_per_mtok REAL NOT NULL,
  source_id INTEGER NOT NULL,
  fetched_ts INTEGER NOT NULL
);

INSERT INTO model_price_v39 SELECT old.model_id, old.input_per_mtok, old.output_per_mtok, old.cache_write_5m_per_mtok, old.cache_write_1h_per_mtok, old.cache_read_per_mtok, (SELECT id FROM dict_price_source WHERE value=old.source), old.fetched_ts FROM model_price old;

DROP TABLE model_price;

ALTER TABLE model_price_v39 RENAME TO model_price;

CREATE TABLE agent_trace_event_v39 (
  event_id INTEGER PRIMARY KEY,
  event_key TEXT NOT NULL UNIQUE,
  lane_id INTEGER NOT NULL REFERENCES dict_session(id),
  trace_id INTEGER REFERENCES dict_trace(id),
  session_id INTEGER REFERENCES dict_session(id),
  from_lane_id INTEGER REFERENCES dict_session(id),
  to_lane_id INTEGER REFERENCES dict_session(id),
  kind_id INTEGER NOT NULL REFERENCES dict_trace_kind(id),
  started_ts INTEGER,
  finished_ts INTEGER,
  delivery_state_id INTEGER REFERENCES dict_trace_delivery(id),
  classification_id INTEGER REFERENCES dict_trace_classification(id),
  detail TEXT NOT NULL DEFAULT '',
  created_ts INTEGER NOT NULL
);

INSERT INTO agent_trace_event_v39 SELECT old.event_id, old.event_key, old.lane_id, old.trace_id, old.session_id, old.from_lane_id, old.to_lane_id, (SELECT id FROM dict_trace_kind WHERE value=old.kind), old.started_ts, old.finished_ts, (SELECT id FROM dict_trace_delivery WHERE value=old.delivery_state), (SELECT id FROM dict_trace_classification WHERE value=old.classification), old.detail, old.created_ts FROM agent_trace_event old;

DROP TABLE agent_trace_event;

ALTER TABLE agent_trace_event_v39 RENAME TO agent_trace_event;

CREATE INDEX IF NOT EXISTS idx_trace_event_lane_time
  ON agent_trace_event(lane_id, created_ts, event_id);

CREATE INDEX IF NOT EXISTS idx_trace_event_trace_time
  ON agent_trace_event(trace_id, created_ts, event_id);

CREATE TABLE agent_live_v39 (
  session_id INTEGER PRIMARY KEY,
  pid INTEGER,
  tmux_pane_id INTEGER,
  status_id INTEGER,
  door_kind TEXT,
  door_addr TEXT,
  last_seen_ts INTEGER,
  pane_alive INTEGER,
  pid_alive INTEGER,
  tmux_session TEXT
);

INSERT INTO agent_live_v39 SELECT old.session_id, old.pid, old.tmux_pane_id, (SELECT id FROM dict_status WHERE value=old.status), old.door_kind, old.door_addr, old.last_seen_ts, old.pane_alive, old.pid_alive, old.tmux_session FROM agent_live old;

DROP TABLE agent_live;

ALTER TABLE agent_live_v39 RENAME TO agent_live;

CREATE TABLE agent_live_span_v39 (
  session_id INTEGER NOT NULL,
  from_ts INTEGER NOT NULL,
  to_ts INTEGER,
  status_id INTEGER NOT NULL,
  pid INTEGER,
  tmux_pane_id INTEGER,
  PRIMARY KEY (session_id, from_ts)
) WITHOUT ROWID;

INSERT INTO agent_live_span_v39 SELECT old.session_id, old.from_ts, old.to_ts, (SELECT id FROM dict_status WHERE value=old.status), old.pid, old.tmux_pane_id FROM agent_live_span old;

DROP TABLE agent_live_span;

ALTER TABLE agent_live_span_v39 RENAME TO agent_live_span;

CREATE INDEX IF NOT EXISTS idx_live_span_open ON agent_live_span(session_id, to_ts);

CREATE TABLE agent_session_v39 (
  session_id INTEGER PRIMARY KEY,
  harness_id INTEGER NOT NULL,
  nickname TEXT,
  cwd_id INTEGER,
  branch_id INTEGER,
  started_ts INTEGER
);

INSERT INTO agent_session_v39 SELECT old.session_id, (SELECT id FROM dict_harness WHERE value=old.harness), old.nickname, old.cwd_id, old.branch_id, old.started_ts FROM agent_session old;

DROP TABLE agent_session;

ALTER TABLE agent_session_v39 RENAME TO agent_session;

CREATE INDEX IF NOT EXISTS idx_session_cwd ON agent_session(cwd_id);

CREATE TABLE agent_lane_v39 (
  spawn_id INTEGER PRIMARY KEY,
  lane_id INTEGER NOT NULL,
  trace_id INTEGER,
  harness_id INTEGER,
  branch_id INTEGER,
  cwd_id INTEGER,
  model_id INTEGER,
  parent_lane_id INTEGER,
  goal TEXT,
  brief_path_id INTEGER,
  brief_markdown_id INTEGER,
  spawned_ts INTEGER NOT NULL
);

INSERT INTO agent_lane_v39 SELECT old.spawn_id, old.lane_id, old.trace_id, (SELECT id FROM dict_harness WHERE value=old.harness), old.branch_id, old.cwd_id, old.model_id, old.parent_lane_id, old.goal, old.brief_path_id, old.brief_markdown_id, old.spawned_ts FROM agent_lane old;

DROP TABLE agent_lane;

ALTER TABLE agent_lane_v39 RENAME TO agent_lane;

CREATE INDEX IF NOT EXISTS idx_lane_trace ON agent_lane(trace_id);

CREATE INDEX IF NOT EXISTS idx_lane_lane ON agent_lane(lane_id, spawned_ts);

CREATE TABLE agent_delivery_v39 (
  message_id TEXT NOT NULL,
  route TEXT NOT NULL,
  harness_id INTEGER,
  outcome TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (message_id, route)
) WITHOUT ROWID;

INSERT INTO agent_delivery_v39 SELECT old.message_id, old.route, (SELECT id FROM dict_harness WHERE value=old.harness), old.outcome, old.detail, old.at_ms FROM agent_delivery old;

DROP TABLE agent_delivery;

ALTER TABLE agent_delivery_v39 RENAME TO agent_delivery;

CREATE TABLE agent_delivery_transition_v39 (
  message_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  route TEXT NOT NULL,
  harness_id INTEGER,
  outcome TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  error_code TEXT,
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (message_id, sequence)
) WITHOUT ROWID;

INSERT INTO agent_delivery_transition_v39 SELECT old.message_id, old.sequence, old.route, (SELECT id FROM dict_harness WHERE value=old.harness), old.outcome, old.detail, old.error_code, old.at_ms FROM agent_delivery_transition old;

DROP TABLE agent_delivery_transition;

ALTER TABLE agent_delivery_transition_v39 RENAME TO agent_delivery_transition;

CREATE INDEX IF NOT EXISTS idx_delivery_transition_route
  ON agent_delivery_transition(message_id, route);

CREATE TABLE sync_root_stamp_v39 (
  harness_id INTEGER NOT NULL,
  root_path_id INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL,
  PRIMARY KEY (harness_id, root_path_id)
) WITHOUT ROWID;

INSERT INTO sync_root_stamp_v39 SELECT (SELECT id FROM dict_harness WHERE value=old.harness), old.root_path_id, old.mtime_ms FROM sync_root_stamp old;

DROP TABLE sync_root_stamp;

ALTER TABLE sync_root_stamp_v39 RENAME TO sync_root_stamp;

CREATE VIEW agent_turn_comment_reply AS SELECT c.comment_id,
       t.session_id,
       t.turn AS target_turn,
       (SELECT MIN(a.turn)
          FROM agent_turn a
          JOIN dict_role r ON r.id = a.role_id
         WHERE a.session_id = t.session_id
           AND a.turn > t.turn
           AND r.value = 'assistant'
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
       role.value AS role,
       turn.said AS body,
       turn.ts,
       cwd.value AS cwd,
       turn.source_class
  FROM agent_turn turn
  JOIN dict_session session ON session.id = turn.session_id
  JOIN dict_role role ON role.id = turn.role_id
  LEFT JOIN agent_session session_row ON session_row.session_id = turn.session_id
  LEFT JOIN dict_cwd cwd ON cwd.id = COALESCE(turn.cwd_id, session_row.cwd_id)
 WHERE role.value IN ('user', 'assistant')
   AND NOT (role.value = 'user' AND turn.source_class = 'harness');

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
       dict_harness.value AS harness,
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
JOIN dict_harness ON dict_harness.id = agent_session.harness_id
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
            AND agent_turn.role_id = (SELECT id FROM dict_role WHERE value = 'user')
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
