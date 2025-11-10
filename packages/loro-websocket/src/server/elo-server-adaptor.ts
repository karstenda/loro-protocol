import type { CrdtServerAdaptor } from "loro-adaptors";
import {
  CrdtType,
  JoinResponseOk,
  MessageType,
  Permission,
  UpdateError,
  UpdateErrorCode,
} from "loro-protocol";
import { EloDoc } from "./elo-doc";

function createInvalidUpdateError(message: string): UpdateError {
  return {
    type: MessageType.UpdateError,
    crdt: CrdtType.Elo,
    roomId: "",
    code: UpdateErrorCode.InvalidUpdate,
    message,
  };
}

export class EloServerAdaptor implements CrdtServerAdaptor {
  readonly crdtType = CrdtType.Elo;

  createEmpty(): Uint8Array {
    return new Uint8Array();
  }

  handleJoinRequest(
    documentData: Uint8Array,
    clientVersion: Uint8Array,
    permission: Permission
  ): {
    response: JoinResponseOk;
    updates?: Uint8Array[];
  } {
    const doc = new EloDoc();
    const load = doc.loadFromEncodedState(documentData);
    if (!load.ok) {
      console.warn("[ELO] failed to load indexed state:", load.error);
    }

    const response: JoinResponseOk = {
      type: MessageType.JoinResponseOk,
      crdt: this.crdtType,
      roomId: "",
      permission,
      version: doc.getVersionBytes(),
    };

    const updates = doc.selectBackfillBatches(clientVersion);

    return {
      response,
      updates: updates.length ? updates : undefined,
    };
  }

  applyUpdates(
    documentData: Uint8Array,
    updates: Uint8Array[],
    permission: Permission
  ): {
    success: boolean;
    newDocumentData?: Uint8Array;
    error?: UpdateError;
    broadcastUpdates?: Uint8Array[];
  } {
    if (permission === "read") {
      return {
        success: false,
        error: {
          type: MessageType.UpdateError,
          crdt: this.crdtType,
          roomId: "",
          code: UpdateErrorCode.PermissionDenied,
          message: "Read-only permission, cannot apply updates",
        },
      };
    }

    const doc = new EloDoc();
    const load = doc.loadFromEncodedState(documentData);
    if (!load.ok) {
      return {
        success: false,
        error: createInvalidUpdateError(load.error),
      };
    }

    const broadcastUpdates: Uint8Array[] = [];

    for (const update of updates) {
      if (!update.length) continue;
      const res = doc.indexBatch(update);
      if (!res.ok) {
        return {
          success: false,
          error: createInvalidUpdateError(res.error),
        };
      }
      broadcastUpdates.push(update);
    }

    const newDocumentData = doc.exportIndexedRecords();

    return {
      success: true,
      newDocumentData,
      broadcastUpdates: broadcastUpdates.length ? broadcastUpdates : undefined,
    };
  }

  getVersion(documentData: Uint8Array): Uint8Array {
    const doc = new EloDoc();
    const load = doc.loadFromEncodedState(documentData);
    if (!load.ok) {
      console.warn("[ELO] failed to load indexed state:", load.error);
      return new Uint8Array();
    }
    return doc.getVersionBytes();
  }

  merge(documents: Uint8Array[]): Uint8Array {
    const doc = new EloDoc();
    for (const data of documents) {
      if (!data.length) continue;
      const res = doc.indexBatch(data);
      if (!res.ok) {
        console.warn("[ELO] failed to merge indexed state:", res.error);
      }
    }
    return doc.exportIndexedRecords();
  }
}
