import {defineConfig} from 'vitest/config';
export default defineConfig({test:{environment:'node',include:['.agent-org/evidence/aggregate-risk.test.ts']}});
