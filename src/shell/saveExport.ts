import { save } from "@tauri-apps/plugin-dialog";
import { messageFor, toCmdError } from "../ipc/errors";
import { revealExported } from "../ipc/exports";
import { useUi } from "../store/ui";

type T = (key: string, values?: Record<string, string>) => string;

type Options = {
  /** The suggested file name. */
  name: string;
  /** Shown in the dialog's type list, e.g. "PDF". */
  type: string;
  extension: string;
  /** Writes the file and returns where it went. */
  write: (path: string) => Promise<string>;
};

/** The last part of a path, for messages. */
export const fileNameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;

/**
 * Asks where to save, writes, and says so with a way to show the file.
 * Cancelling the dialog writes nothing and says nothing.
 */
export async function saveExport(t: T, { name, type, extension, write }: Options): Promise<void> {
  const { toast } = useUi.getState();
  try {
    const path = await save({
      defaultPath: name,
      filters: [{ name: type, extensions: [extension] }],
    });
    if (!path) return;
    const written = await write(path);
    toast("info", t("Saved: {name}", { name: fileNameOf(written) }), {
      label: t("Show in folder"),
      run: () => {
        revealExported(written).catch((error) => toast("error", messageFor(toCmdError(error), t)));
      },
    });
  } catch (error) {
    toast("error", messageFor(toCmdError(error), t));
  }
}
