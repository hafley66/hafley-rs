CREATE TABLE IF NOT EXISTS agent_cmd (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  program_id INTEGER NOT NULL,
  argline TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_commit_push (
  lane TEXT NOT NULL,
  subscriber TEXT NOT NULL,
  head TEXT NOT NULL,
  message_id TEXT NOT NULL,
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (lane, subscriber, head)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_commit_subscription (
  subscriber TEXT NOT NULL,
  lane TEXT NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('door', 'mailbox')),
  created_at TEXT NOT NULL,
  PRIMARY KEY (subscriber, lane)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_delivery" (
  message_id TEXT NOT NULL,
  route TEXT NOT NULL,
  harness TEXT CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  outcome TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  at_ms INTEGER NOT NULL,
  PRIMARY KEY (message_id, route)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_delivery_transition" (
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
CREATE TABLE IF NOT EXISTS agent_door_blowout (
  blowout_id INTEGER PRIMARY KEY,
  route TEXT NOT NULL,
  at_ms INTEGER NOT NULL,
  pushes INTEGER NOT NULL,
  budget INTEGER NOT NULL,
  window_ms INTEGER NOT NULL,
  cooldown_ms INTEGER NOT NULL,
  why TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS "agent_edge" (
  parent_session_id INTEGER NOT NULL,
  child_session_id INTEGER NOT NULL,
  edge_kind TEXT NOT NULL CHECK (edge_kind IN ('spawned','result','deliver-nextturn','hail','deliver-midturn','completed','completion-mailed','completion-delivered','cancel')),
  agent_type_id INTEGER,
  model_id INTEGER,
  first_ts INTEGER,
  last_ts INTEGER,
  n INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (parent_session_id, child_session_id, edge_kind)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_favorite (
  favorite_id INTEGER PRIMARY KEY,
  markdown_id INTEGER NOT NULL,
  note TEXT,
  source_kind TEXT NOT NULL DEFAULT 'empty' CHECK (source_kind IN ('empty','text','session','agent_session','codex','turn','turn_range','missing_turn')),
  source_session TEXT,
  source_turn INTEGER,
  source_turn_end INTEGER,
  source_harness TEXT,
  source_role TEXT,
  source_codex_ref TEXT,
  source_text TEXT NOT NULL DEFAULT '',
  created_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS "agent_fetch" (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  kind TEXT NOT NULL CHECK (kind IN ('fetch','search')),
  url_id INTEGER,
  domain_id INTEGER,
  query TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_lane" (
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
CREATE TABLE IF NOT EXISTS agent_lane_head (
  lane TEXT PRIMARY KEY,
  reported_head TEXT NOT NULL,
  at_ms INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_live" (
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
CREATE TABLE IF NOT EXISTS "agent_live_span" (
  session_id INTEGER NOT NULL,
  from_ts INTEGER NOT NULL,
  to_ts INTEGER,
  status TEXT NOT NULL CHECK (status IN ('idle','live','detached','dead','closed')),
  pid INTEGER,
  tmux_pane_id INTEGER,
  PRIMARY KEY (session_id, from_ts)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_mail (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  message_id TEXT NOT NULL UNIQUE,
  mailbox TEXT NOT NULL DEFAULT 'bus',
  from_route TEXT NOT NULL,
  to_route TEXT NOT NULL,
  from_timestamp TEXT NOT NULL,
  to_timestamp TEXT,
  kind TEXT NOT NULL,
  reply_to TEXT,
  body TEXT NOT NULL,
  ref_id TEXT,
  rc INTEGER,
  detail TEXT
);
CREATE TABLE IF NOT EXISTS "agent_pr" (
                   session_id INTEGER NOT NULL,
                   turn INTEGER NOT NULL,
                   pr_url_id INTEGER NOT NULL,
                   PRIMARY KEY (session_id, turn, pr_url_id)
                 ) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_pr_notice (
  pr_url TEXT PRIMARY KEY,
  lane TEXT NOT NULL,
  at_ms INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_reminder (
 name TEXT PRIMARY KEY, route TEXT NOT NULL, body TEXT NOT NULL,
 every_ms INTEGER NOT NULL CHECK(every_ms > 0), until_ms INTEGER NOT NULL,
 next_ms INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'active',
 last_message TEXT, detail TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS agent_route (
  route TEXT PRIMARY KEY,
  kind TEXT NOT NULL DEFAULT 'lane',
  harness TEXT,
  tmux TEXT,
  cwd TEXT,
  model TEXT,
  mode TEXT,
  session_id TEXT,
  source_path TEXT,
  parent TEXT,
  goal TEXT,
  registered_at TEXT,
  base_sha TEXT,
  worktree_dir TEXT,
  app_server_socket TEXT
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_route_selection (
            route TEXT PRIMARY KEY REFERENCES agent_route(route) ON DELETE CASCADE,
            last_focused_at INTEGER,
            selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0,1))
        ) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_session" (
  session_id INTEGER PRIMARY KEY,
  harness TEXT NOT NULL CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  nickname TEXT,
  cwd_id INTEGER,
  branch_id INTEGER,
  started_ts INTEGER
);
CREATE TABLE IF NOT EXISTS "agent_session_attr" (
  session_id INTEGER NOT NULL,
  key TEXT NOT NULL CHECK (key IN ('effort','reset_ts','process_pid','process_start_secs','process_previous_session','mood')),
  value TEXT NOT NULL,
  set_ts INTEGER NOT NULL,
  PRIMARY KEY (session_id, key)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_session_observation" (
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
  source TEXT NOT NULL CHECK (source IN ('legacy-agent-trace-span','trace-event','transcript-sync','trace-attach','live-status','transcript-session-metadata'))
);
CREATE TABLE IF NOT EXISTS "agent_session_relation" (
  relation_id INTEGER PRIMARY KEY,
  relation_key TEXT NOT NULL UNIQUE,
  from_session INTEGER NOT NULL REFERENCES dict_session(id),
  to_session INTEGER NOT NULL REFERENCES dict_session(id),
  kind TEXT NOT NULL CHECK (kind IN ('continued-in','same-process','parent-child','same-pane','same-tui')),
  source TEXT NOT NULL CHECK (source IN ('legacy-agent-trace-span','trace-event','transcript-sync','trace-attach','live-status','transcript-session-metadata')),
  observed_ts INTEGER NOT NULL,
  matched_identity_key TEXT
);
CREATE TABLE IF NOT EXISTS agent_skill (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  skill_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_span (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  line_start INTEGER,
  line_end INTEGER,
  PRIMARY KEY (session_id, turn, path_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_tag (
  tag TEXT PRIMARY KEY,
  created_ts INTEGER NOT NULL,
  last_used_ts INTEGER NOT NULL,
  uses INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_tag_link (
  tag TEXT NOT NULL,
  source TEXT NOT NULL,
  ts INTEGER NOT NULL,
  PRIMARY KEY (tag, source)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "agent_touch" (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  path_id INTEGER NOT NULL,
  verb TEXT NOT NULL CHECK (verb IN ('read','Read','edit','Edit','write','Write','grep','Grep','glob','Glob')),
  raw_verb TEXT CHECK (raw_verb IN ('read','Read','edit','Edit','write','Write','grep','Grep','glob','Glob')),
  PRIMARY KEY (session_id, turn, path_id, verb)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_trace (
  trace_id INTEGER PRIMARY KEY,
  root_session_id INTEGER,
  started_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS "agent_trace_event" (
  event_id INTEGER PRIMARY KEY,
  event_key TEXT NOT NULL UNIQUE,
  lane_id INTEGER NOT NULL REFERENCES dict_session(id),
  trace_id INTEGER REFERENCES dict_trace(id),
  session_id INTEGER REFERENCES dict_session(id),
  from_lane_id INTEGER REFERENCES dict_session(id),
  to_lane_id INTEGER REFERENCES dict_session(id),
  kind TEXT NOT NULL CHECK (kind IN ('supervisor-start','channel-open','turn-start','error','supervisor-exit','turn-finish','delivery','idle-shutdown','cli-invocation','harness-quiet','harness-active','session-boundary')),
  started_ts INTEGER,
  finished_ts INTEGER,
  delivery_state TEXT CHECK (delivery_state IN ('midturn','nextturn','started','ok','version','help','parse-error','error')),
  classification TEXT CHECK (classification IN ('starting','opened','started','failed','completed','retryable','delivered','queued','retired','quiet','active','same-process')),
  detail TEXT NOT NULL DEFAULT '',
  created_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS "agent_trace_span" (
  session_id INTEGER PRIMARY KEY,
  trace_id INTEGER NOT NULL,
  attach TEXT NOT NULL CHECK (attach IN ('backfill-spawned-edge','lane-create','lane-run','supervisor-conversation','native-tui-session','derived-session-relation')),
  attached_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS "agent_turn" (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  role TEXT NOT NULL CHECK (role IN ('user','assistant','tool','system','developer','meta')),
  said TEXT,
  cwd_id INTEGER,
  source_class TEXT NOT NULL DEFAULT 'unknown',
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_turn_comment (
  comment_id INTEGER PRIMARY KEY,
  client_id TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL DEFAULT 'selection',
  quote TEXT NOT NULL,
  note TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  tab_name TEXT,
  created_ts INTEGER NOT NULL,
  updated_ts INTEGER NOT NULL,
  sent_ts INTEGER
);
CREATE TABLE IF NOT EXISTS agent_turn_comment_fork (
  comment_id INTEGER NOT NULL,
  lane TEXT NOT NULL,
  branch TEXT NOT NULL,
  brief TEXT NOT NULL,
  created_ts INTEGER NOT NULL,
  PRIMARY KEY (comment_id, lane)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_turn_comment_target (
  comment_id INTEGER NOT NULL,
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  PRIMARY KEY (comment_id, session_id, turn)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS agent_usage (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  request_ref INTEGER NOT NULL,
  model_id INTEGER NOT NULL,
  service_tier_id INTEGER,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_5m_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_1h_tokens INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens INTEGER NOT NULL DEFAULT 0,
  is_sidechain INTEGER NOT NULL DEFAULT 0,
  cost_usd_recorded REAL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS dict_agenttype (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_branch (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_cwd (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_domain (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_model (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_pane (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_path (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_pr (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_program (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_record (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_request (
  id INTEGER PRIMARY KEY,
  message_id TEXT NOT NULL,
  request_id TEXT NOT NULL DEFAULT '',
  UNIQUE (message_id, request_id)
);
CREATE TABLE IF NOT EXISTS dict_service_tier (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_session (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_skill (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_trace (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_tui_session (
  id INTEGER PRIMARY KEY,
  value TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS dict_url (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS mail_import (
  path TEXT PRIMARY KEY,
  offset INTEGER NOT NULL DEFAULT 0,
  digest TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS markdown_cache (
  markdown_id INTEGER PRIMARY KEY,
  digest TEXT NOT NULL UNIQUE,
  body TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  first_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS "model_price" (
  model_id INTEGER PRIMARY KEY,
  input_per_mtok REAL NOT NULL,
  output_per_mtok REAL NOT NULL,
  cache_write_5m_per_mtok REAL NOT NULL,
  cache_write_1h_per_mtok REAL NOT NULL,
  cache_read_per_mtok REAL NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('litellm','openrouter')),
  fetched_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS mood (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE CHECK (name IN ('plain','unga','board')),
  template TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_cursor (
  session_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  offset INTEGER NOT NULL, record_id_id INTEGER, turn INTEGER NOT NULL DEFAULT 0, timestamp INTEGER NOT NULL DEFAULT 0, modified_ms INTEGER NOT NULL DEFAULT 0, projection_version INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (session_id, path_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS "sync_root_stamp" (
  harness TEXT NOT NULL CHECK (harness IN ('claude','codex','kimi','opencode','gemini','omp')),
  root_path_id INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL,
  PRIMARY KEY (harness, root_path_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_commit_push_subscriber
  ON agent_commit_push(subscriber, at_ms);
CREATE INDEX IF NOT EXISTS idx_delivery_transition_route
  ON agent_delivery_transition(message_id, route);
CREATE INDEX IF NOT EXISTS idx_door_blowout_route
  ON agent_door_blowout(route, at_ms);
CREATE INDEX IF NOT EXISTS idx_edge_child ON agent_edge(child_session_id, edge_kind);
CREATE INDEX IF NOT EXISTS idx_fetch_ts ON agent_fetch(ts);
CREATE INDEX IF NOT EXISTS idx_lane_lane ON agent_lane(lane_id, spawned_ts);
CREATE INDEX IF NOT EXISTS idx_lane_trace ON agent_lane(trace_id);
CREATE INDEX IF NOT EXISTS idx_live_span_open ON agent_live_span(session_id, to_ts);
CREATE INDEX IF NOT EXISTS idx_mail_from ON agent_mail(from_route, seq);
CREATE INDEX IF NOT EXISTS idx_mail_to ON agent_mail(to_route, seq);
CREATE UNIQUE INDEX IF NOT EXISTS idx_reminder_active_route
 ON agent_reminder(route) WHERE state = 'active';
CREATE INDEX IF NOT EXISTS idx_session_cwd ON agent_session(cwd_id);
CREATE INDEX IF NOT EXISTS idx_session_observation_session_time
  ON agent_session_observation(session_id, observed_ts);
CREATE INDEX IF NOT EXISTS idx_session_observation_source_time
  ON agent_session_observation(source, observed_ts);
CREATE INDEX IF NOT EXISTS idx_session_relation_from ON agent_session_relation(from_session);
CREATE INDEX IF NOT EXISTS idx_session_relation_source_time
  ON agent_session_relation(source, observed_ts);
CREATE INDEX IF NOT EXISTS idx_session_relation_to ON agent_session_relation(to_session);
CREATE INDEX IF NOT EXISTS idx_span_trace ON agent_trace_span(trace_id);
CREATE INDEX IF NOT EXISTS idx_tag_link_source ON agent_tag_link(source);
CREATE INDEX IF NOT EXISTS idx_touch_pathid ON agent_touch(path_id);
CREATE INDEX IF NOT EXISTS idx_trace_event_lane_time
  ON agent_trace_event(lane_id, created_ts, event_id);
CREATE INDEX IF NOT EXISTS idx_trace_event_trace_time
  ON agent_trace_event(trace_id, created_ts, event_id);
CREATE INDEX IF NOT EXISTS idx_turn_comment_target
  ON agent_turn_comment_target(session_id, turn);
CREATE INDEX IF NOT EXISTS idx_turn_session_ts ON agent_turn(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_usage_model_ts ON agent_usage(model_id, ts);
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_request ON agent_usage(request_ref);
CREATE INDEX IF NOT EXISTS idx_usage_ts ON agent_usage(ts);
CREATE VIEW IF NOT EXISTS agent_turn_comment_reply AS SELECT c.comment_id,
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
CREATE VIEW IF NOT EXISTS v_conversational_turn AS SELECT turn.session_id,
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
CREATE VIEW IF NOT EXISTS v_favorite AS SELECT favorite.favorite_id AS id, favorite.note, markdown.body,
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
CREATE VIEW IF NOT EXISTS v_message AS SELECT current.session,
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
CREATE VIEW IF NOT EXISTS v_session_project AS WITH projects AS (
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
CREATE VIEW IF NOT EXISTS v_skill_cost_act AS SELECT dict_skill.value AS skill,
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
CREATE VIEW IF NOT EXISTS v_skill_cost_window AS WITH act AS (
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
CREATE VIEW IF NOT EXISTS v_turn_cwd AS SELECT t.session_id,
       t.turn,
       cwd.value AS cwd
  FROM agent_turn t
  JOIN agent_session s ON s.session_id = t.session_id
  LEFT JOIN dict_cwd cwd ON cwd.id = COALESCE(t.cwd_id, s.cwd_id);
CREATE VIEW IF NOT EXISTS v_usage_cost AS SELECT usage.session_id,
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
CREATE TRIGGER IF NOT EXISTS agent_mail_needs_transition
BEFORE INSERT ON agent_mail
WHEN NOT EXISTS (
  SELECT 1 FROM agent_delivery_transition WHERE message_id = NEW.message_id
)
BEGIN
  SELECT RAISE(ABORT, 'agent_mail row without a delivery transition');
END;
CREATE TRIGGER IF NOT EXISTS agent_route_selection_delete
        AFTER DELETE ON agent_route BEGIN
            DELETE FROM agent_route_selection WHERE route=OLD.route;
        END;
