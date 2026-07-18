// Starts one real `cargo-cratevista serve` process per snapshot and publishes
// their base URLs to the test workers via the environment. Returns a teardown so
// the processes are reaped even when the run fails.
import { cleanWorkspaces, startServer, type ServerHandle } from "./support/harness";

export default async function globalSetup(): Promise<() => Promise<void>> {
  const servers: ServerHandle[] = [];
  try {
    const normal = await startServer("normal");
    servers.push(normal);
    process.env.CRATEVISTA_E2E_NORMAL_URL = normal.baseURL;

    const partial = await startServer("partial");
    servers.push(partial);
    process.env.CRATEVISTA_E2E_PARTIAL_URL = partial.baseURL;
  } catch (error) {
    await Promise.all(servers.map((server) => server.stop()));
    cleanWorkspaces();
    throw error;
  }

  return async () => {
    await Promise.all(servers.map((server) => server.stop()));
    cleanWorkspaces();
  };
}
