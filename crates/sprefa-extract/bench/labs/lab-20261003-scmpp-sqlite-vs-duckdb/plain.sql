SELECT (SELECT p.text FROM scmpp_dict_path AS p WHERE p.id = "r0".file) AS path,
  "c0_call"."start" AS "call__start",
  "c0_call"."end" AS "call__end",
  (SELECT t.text FROM scmpp_dict_text AS t WHERE t.id = "c0_call".text) AS "call__text"
FROM scmpp_capture AS "r0"
  LEFT JOIN scmpp_capture AS "c0_call" ON "c0_call".file = "r0".file AND "c0_call".pattern = 0 AND "c0_call"."match" = "r0"."match" AND "c0_call".capture = 2
WHERE "r0".pattern = 0
  AND "r0".capture = 1
ORDER BY path, "r0".start, "r0"."match"
