#!/bin/bash
set -euo pipefail
CASE=D08
source "$(dirname "${BASH_SOURCE[0]}")/1_type_errors.sh"
type_baseline signals
"$RYII" cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts --state "$STATE" --commit > .dogfood/D08.signal
rg '^(import|export).*2a_isSignal\.js["\x27]$' packages/signals/src/2_Signal.ts
! rg '^(import|export).*2a_isSignal.*;' packages/signals/src/2_Signal.ts
cat > packages/signals/src/2b_importStyle.ts <<'TS'
import { isSignal, toSignal, Signal } from './2_Signal.js'
export const styleProbe = [isSignal, toSignal, Signal]
TS
"$RYII" cleave packages/signals/src/2_Signal.ts#toSignal packages/signals/src/2c_toSignal.ts --root "$PWD" --state "$STATE" --commit > .dogfood/D08.group
rg "^import \{ isSignal, Signal \} from './2_Signal.js'$" packages/signals/src/2b_importStyle.ts
rg "^import \{ toSignal \} from './2c_toSignal.js'$" packages/signals/src/2b_importStyle.ts
type_no_new signals
echo 'D8: cleave keeps .js suffix, quote, semicolon style and grouped named imports'
