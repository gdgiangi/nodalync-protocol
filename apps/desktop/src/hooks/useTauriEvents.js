import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

/**
 * Hook to subscribe to Tauri backend events.
 * Auto-cleans up on unmount. Stable callback refs prevent re-subscriptions.
 *
 * Usage:
 *   useTauriEvents({
 *     "graph:updated": (payload) => mergeGraphDiff(payload),
 *     "ai:progress": (payload) => setProgress(payload),
 *   });
 */
export function useTauriEvents(eventHandlers) {
  const handlersRef = useRef(eventHandlers);
  handlersRef.current = eventHandlers;

  useEffect(() => {
    const unlisteners = [];

    for (const eventName of Object.keys(handlersRef.current)) {
      const promise = listen(eventName, (event) => {
        handlersRef.current[eventName]?.(event.payload);
      });
      unlisteners.push(promise);
    }

    return () => {
      // Resolve all listen promises and call their unlisten functions
      Promise.all(unlisteners).then((fns) => {
        fns.forEach((unlisten) => unlisten());
      });
    };
    // Only re-subscribe if the set of event names changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [Object.keys(eventHandlers).sort().join(",")]);
}

/**
 * Hook for a single Tauri event.
 *
 * Usage:
 *   useTauriEvent("graph:updated", (payload) => mergeGraphDiff(payload));
 */
export function useTauriEvent(eventName, handler) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    const promise = listen(eventName, (event) => {
      handlerRef.current?.(event.payload);
    });

    return () => {
      promise.then((unlisten) => unlisten());
    };
  }, [eventName]);
}
