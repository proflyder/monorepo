// bb-plugin-profiles — frontend: a profile switcher in the sidebar footer, a
// header hook that remembers the last thread per profile, and a "Profiles"
// page that assigns every project to a profile. The thread list itself is bb's
// built-in one; the server hides the other profiles' projects in it.
import { useCallback, useEffect, useRef, useState } from "react";
import {
  definePluginApp,
  experimental_useSidebarThreads as useSidebarThreads,
  useBbNavigate,
  useRealtime,
  useRpc,
  type PluginThreadHeaderActionProps,
} from "@get-bb/plugin-sdk/app";
import type { rpcContract, ProjectRow } from "./server";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const selectClass =
  "h-7 rounded-md border border-border bg-background px-2 text-xs text-foreground";

// Last known state, kept across mounts (the footer popover and the page remount
// on every open) and across reloads, so they render at once and refresh quietly.
interface StateCache {
  profiles: string[];
  active: string;
  folderPicker: boolean;
  projects: ProjectRow[];
}
const CACHE_KEY = "bb-profiles-cache";
let cache: StateCache = (() => {
  try {
    const stored = JSON.parse(window.localStorage.getItem(CACHE_KEY) ?? "null");
    if (stored !== null && Array.isArray(stored.profiles)) {
      return { projects: [], folderPicker: false, ...stored } as StateCache;
    }
  } catch {
    // fall through to the empty cache
  }
  return { profiles: [], active: "work", folderPicker: false, projects: [] };
})();
function updateCache(patch: Partial<StateCache>) {
  cache = { ...cache, ...patch };
  window.localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
}

function useProfiles() {
  const rpc = useRpc<typeof rpcContract>();
  const [profiles, setProfiles] = useState<string[]>(cache.profiles);
  const [active, setActive] = useState(cache.active);
  const [folderPicker, setFolderPicker] = useState(cache.folderPicker);
  const [error, setError] = useState<string | null>(null);
  const refetch = useCallback(() => {
    rpc.call("state").then(
      (state) => {
        updateCache({
          profiles: state.profiles,
          active: state.active,
          folderPicker: state.folderPicker,
        });
        setProfiles(state.profiles);
        setActive(state.active);
        setFolderPicker(state.folderPicker);
      },
      (cause) => setError(String(cause)),
    );
  }, [rpc]);
  useEffect(refetch, [refetch]);
  useRealtime("profiles-changed", refetch);
  return { rpc, profiles, active, folderPicker, error, setError, refetch };
}

// Sidebar footer popover: pick the profile whose projects the list shows.
function ProfileSwitcher({ dismiss }: { dismiss(): void }) {
  const { rpc, profiles, active, setError, error } = useProfiles();
  const { threads } = useSidebarThreads();
  const navigate = useBbNavigate();
  const switchTo = (name: string) => {
    rpc
      .call("setActive", { name })
      .then(() => rpc.call("lastThread", { profile: name }))
      .then(({ threadId }) => {
        const remembered =
          threadId === null
            ? undefined
            : threads.find((t) => t.id === threadId && !t.isArchived);
        if (remembered !== undefined) navigate.toThread(remembered.id);
        dismiss();
      })
      .catch((cause) => setError(String(cause)));
  };
  // With exactly two profiles the icon toggles between them: no popover.
  const toggleTarget =
    profiles.length === 2 ? profiles.find((name) => name !== active) : undefined;
  const toggled = useRef(false);
  useEffect(() => {
    if (toggleTarget === undefined || toggled.current) return;
    toggled.current = true;
    switchTo(toggleTarget);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [toggleTarget]);
  if (toggleTarget !== undefined) return null;
  return (
    <div className="flex flex-col gap-1 p-2">
      <div className="px-1 text-xs font-medium text-muted-foreground">Profile</div>
      {profiles.map((name) => (
        <button
          key={name}
          type="button"
          onClick={() => switchTo(name)}
          className={
            "h-8 rounded-md px-2 text-left text-sm " +
            (name === active
              ? "bg-accent text-accent-foreground"
              : "hover:bg-accent/50")
          }
        >
          {name}
        </button>
      ))}
      <button
        type="button"
        onClick={() => {
          navigate.toPluginPanel("profiles");
          dismiss();
        }}
        className="h-8 rounded-md px-2 text-left text-xs text-muted-foreground hover:bg-accent/50"
      >
        Manage profiles…
      </button>
      {error === null ? null : (
        <p role="alert" className="px-1 text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}

// Mounted in every thread header, draws nothing: records the open thread as the
// last one of its project's profile.
function RememberThread({ threadId, projectId }: PluginThreadHeaderActionProps) {
  const rpc = useRpc<typeof rpcContract>();
  useEffect(() => {
    rpc.call("rememberThread", { projectId, threadId }).catch(() => undefined);
  }, [rpc, projectId, threadId]);
  return null;
}

function ProfilesPage() {
  const { rpc, profiles, active, folderPicker, error, setError, refetch } = useProfiles();
  const [projects, setProjects] = useState<ProjectRow[]>(cache.projects);
  const [name, setName] = useState("");
  const [projectName, setProjectName] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [projectProfile, setProjectProfile] = useState<string | null>(null);
  const navigate = useBbNavigate();
  const choosePath = () => {
    rpc.call("pickFolder", null).then(
      ({ path }) => {
        if (path === null) return;
        setProjectPath(path);
        setProjectName((current) =>
          current.trim() === "" ? (path.split("/").pop() ?? "") : current,
        );
        setError(null);
      },
      (cause) => setError(String(cause)),
    );
  };
  const load = useCallback(() => {
    rpc.call("listProjects").then(
      (result) => {
        updateCache({ projects: result.projects });
        setProjects(result.projects);
      },
      (cause) => setError(String(cause)),
    );
  }, [rpc, setError]);
  useEffect(load, [load]);
  useRealtime("profiles-changed", load);
  return (
    <div className="h-full min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto box-border w-full max-w-3xl px-4 pb-4 pt-3 md:px-5 md:pt-4">
        <p className="text-sm text-muted-foreground">
          Each profile has its own provider logins, settings, memory and
          history. A project uses its profile from the next message on. The
          sidebar shows only the projects of the active profile.
        </p>
        <form
          className="mt-4 flex items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            rpc.call("createProfile", { name: name.trim() }).then(
              () => {
                setName("");
                setError(null);
                refetch();
              },
              (cause) => setError(String(cause)),
            );
          }}
        >
          <Input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="New profile name"
            aria-label="New profile name"
          />
          <Button type="submit" disabled={name.trim() === ""}>
            Create
          </Button>
        </form>
        <form
          className="mt-4 flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            rpc
              .call("addProject", {
                name: projectName.trim(),
                path: projectPath.trim(),
                profile: projectProfile ?? active,
              })
              .then(
                (result) => {
                  setProjectName("");
                  setProjectPath("");
                  setError(null);
                  load();
                  navigate.toProject(result.projectId);
                },
                (cause) => setError(String(cause)),
              );
          }}
        >
          <Input
            className="min-w-32 flex-1"
            value={projectName}
            onChange={(event) => setProjectName(event.target.value)}
            placeholder="Project name"
            aria-label="Project name"
          />
          <Input
            className="min-w-48 flex-[2]"
            value={projectPath}
            readOnly={folderPicker}
            onClick={folderPicker ? choosePath : undefined}
            onChange={
              folderPicker
                ? undefined
                : (event) => setProjectPath(event.target.value)
            }
            placeholder={
              folderPicker
                ? "Choose a folder…"
                : "Absolute path on the provider machine"
            }
            aria-label="Project folder"
          />
          {folderPicker ? (
            <Button type="button" variant="outline" onClick={choosePath}>
              Choose…
            </Button>
          ) : null}
          <select
            className={selectClass}
            aria-label="Profile for the new project"
            value={projectProfile ?? active}
            onChange={(event) => setProjectProfile(event.target.value)}
          >
            {profiles.map((profile) => (
              <option key={profile} value={profile}>
                {profile}
              </option>
            ))}
          </select>
          <Button
            type="submit"
            disabled={projectName.trim() === "" || projectPath.trim() === ""}
          >
            Add project
          </Button>
        </form>
        {error === null ? null : (
          <p role="alert" className="mt-3 text-sm text-destructive">
            {error}
          </p>
        )}
        <ul className="mt-4 divide-y divide-border overflow-hidden rounded-lg border border-border bg-card px-4">
          {projects.map((project) => (
            <li key={project.id} className="flex items-center gap-3 py-2.5 text-sm">
              <span className="min-w-0 flex-1 truncate">{project.name}</span>
              <select
                className={selectClass}
                aria-label={`Profile for ${project.name}`}
                value={project.profile}
                onChange={(event) => {
                  rpc
                    .call("setProjectProfile", {
                      projectId: project.id,
                      profile: event.target.value,
                    })
                    .then(load, (cause) => setError(String(cause)));
                }}
              >
                {profiles.map((profile) => (
                  <option key={profile} value={profile}>
                    {profile}
                  </option>
                ))}
              </select>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

export default definePluginApp((app) => {
  app.slots.navPanel({
    id: "profiles",
    title: "Profiles",
    icon: "UserRound",
    path: "profiles",
    component: ProfilesPage,
  });
  app.experimental_sidebarFooter.register({
    kind: "disclosure",
    id: "profile-switch",
    label: "Switch profile",
    icon: "Repeat",
    component: ProfileSwitcher,
  });
  app.slots.experimental_threadHeaderAction({
    id: "remember-thread",
    title: "Profile",
    component: RememberThread,
  });
});
