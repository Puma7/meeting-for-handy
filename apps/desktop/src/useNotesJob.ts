import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "./api";
import type {
  AnalysisResult,
  DesktopApi,
  NotesProgress,
  NotesRequest,
} from "./types";

export type JobFeedback = {
  kind: "complete" | "cancelled" | "failed" | "cancel-unavailable";
  meetingId: string;
  message?: string;
};
export function useNotesJob(
  api: DesktopApi,
  connected: boolean,
  backendRunning: boolean,
  onResult: (result: AnalysisResult) => void,
) {
  const [progress, setProgress] = useState<NotesProgress | null>(null),
    [localBusy, setLocalBusy] = useState(false),
    [cancelPending, setCancelPending] = useState(false),
    [feedback, setFeedback] = useState<JobFeedback | null>(null),
    [elapsedMs, setElapsedMs] = useState(0),
    [pollError, setPollError] = useState<string | null>(null);
  const expected = useRef<string | null>(null),
    lastRequest = useRef<NotesRequest | null>(null),
    started = useRef<number | null>(null),
    mounted = useRef(true),
    resultCallback = useRef(onResult),
    observed = useRef<string | null>(null),
    dismissed = useRef<string | null>(null),
    localBusyRef = useRef(false),
    progressRef = useRef<NotesProgress | null>(null);
  const forgotten = useRef(new Set<string>());
  resultCallback.current = onResult;
  progressRef.current = progress;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const applyProgress = useCallback((next: NotesProgress | null) => {
    if (!mounted.current || !next || forgotten.current.has(next.meetingId))
      return;
    if (
      localBusyRef.current &&
      expected.current &&
      next.jobId !== expected.current
    )
      return;
    const current = progressRef.current;
    if (
      current?.jobId === next.jobId &&
      ((current.status === "complete" && next.status !== "complete") ||
        (current.status !== "running" && next.status === "running") ||
        (next.status === "running" && next.elapsedMs < current.elapsedMs))
    )
      return;
    if (next.status === "running") {
      observed.current = next.jobId;
      dismissed.current = null;
      if (!started.current)
        started.current = Date.now() - Math.max(0, next.elapsedMs);
    }
    if (dismissed.current === next.jobId) return;
    if (
      next.status !== "running" &&
      next.jobId !== observed.current &&
      next.jobId !== expected.current
    )
      return;
    progressRef.current = next;
    setProgress(next);
    setCancelPending(next.cancelRequested && next.status === "running");
    setElapsedMs((previous) =>
      Math.max(
        previous,
        0,
        next.elapsedMs,
        next.status !== "running" || started.current === null
          ? 0
          : Date.now() - started.current,
      ),
    );
    if (next.status !== "running") started.current = null;
    setPollError(null);
    if (next.status === "cancelled")
      setFeedback({ kind: "cancelled", meetingId: next.meetingId });
    else if (next.status === "failed")
      setFeedback({
        kind: "failed",
        meetingId: next.meetingId,
        message: next.error ?? undefined,
      });
    else if (next.status === "complete")
      setFeedback({ kind: "complete", meetingId: next.meetingId });
  }, []);
  const poll = useCallback(async () => {
    try {
      const next = await api.getNotesProgress();
      applyProgress(next);
    } catch (failure) {
      if (
        mounted.current &&
        (backendRunning ||
          localBusyRef.current ||
          progressRef.current?.status === "running")
      )
        setPollError(errorMessage(failure));
    }
  }, [api, applyProgress, backendRunning]);
  const busy = localBusy || backendRunning || progress?.status === "running";
  useEffect(() => {
    if (!connected) return;
    void poll();
    if (!busy) return;
    const timer = setInterval(() => void poll(), 1000);
    return () => clearInterval(timer);
  }, [connected, busy, poll]);
  useEffect(() => {
    if (progress?.status !== "running") return;
    const timer = setInterval(() => {
      if (started.current !== null)
        setElapsedMs((previous) =>
          Math.max(previous, 0, Date.now() - (started.current ?? Date.now())),
        );
    }, 1000);
    return () => clearInterval(timer);
  }, [progress?.status]);
  const run = useCallback(
    async (request: NotesRequest) => {
      if (localBusyRef.current || progressRef.current?.status === "running")
        return;
      const fresh = { ...structuredClone(request), jobId: crypto.randomUUID() };
      forgotten.current.delete(fresh.meetingId);
      expected.current = fresh.jobId;
      observed.current = fresh.jobId;
      dismissed.current = null;
      lastRequest.current = fresh;
      started.current = Date.now();
      localBusyRef.current = true;
      setFeedback(null);
      setPollError(null);
      setLocalBusy(true);
      setCancelPending(false);
      setElapsedMs(0);
      applyProgress({
        jobId: fresh.jobId,
        runId: fresh.resumeRunId ?? null,
        meetingId: fresh.meetingId,
        status: "running",
        stage: "preparing",
        stepName: null,
        stepIndex: 0,
        totalSteps: fresh.template.steps.length,
        batchIndex: 0,
        totalBatches: 0,
        confirmedTasks: 0,
        totalTasks: 0,
        elapsedMs: 0,
        error: null,
        cancelRequested: false,
      });
      try {
        const result = await api.generateNotes(fresh);
        if (!mounted.current || forgotten.current.has(fresh.meetingId)) return;
        if (result.meetingId !== fresh.meetingId)
          throw new Error(
            "Die Auswertung gehört zu einer anderen Sitzung. / Analysis belongs to another meeting.",
          );
        resultCallback.current(result);
        setFeedback({ kind: "complete", meetingId: fresh.meetingId });
        const confirmed = progressRef.current;
        if (confirmed?.jobId === fresh.jobId)
          applyProgress({
            ...confirmed,
            status: "complete",
            stage: "finished",
            runId: result.id,
            cancelRequested: false,
            elapsedMs: Date.now() - (started.current ?? Date.now()),
          });
        const latest = await api.getNotesProgress().catch(() => null);
        if (latest?.jobId === fresh.jobId) applyProgress(latest);
      } catch (failure) {
        if (!mounted.current || forgotten.current.has(fresh.meetingId)) return;
        const latest = await api.getNotesProgress().catch(() => null);
        if (latest?.jobId === fresh.jobId) {
          applyProgress(latest);
          if (latest.status === "running") setPollError(errorMessage(failure));
        } else {
          setFeedback({
            kind: "failed",
            meetingId: fresh.meetingId,
            message: errorMessage(failure),
          });
          const current = progressRef.current;
          if (current?.jobId === fresh.jobId)
            applyProgress({
              ...current,
              status: "failed",
              stage: "finished",
              error: errorMessage(failure),
            });
        }
      } finally {
        localBusyRef.current = false;
        if (mounted.current) {
          setLocalBusy(false);
        }
      }
    },
    [api, applyProgress],
  );
  const cancel = useCallback(async () => {
    const current = progressRef.current;
    if (
      !current ||
      current.status !== "running" ||
      current.cancelRequested ||
      cancelPending
    )
      return;
    setCancelPending(true);
    try {
      const accepted = await api.cancelNotes(current.jobId);
      if (!mounted.current) return;
      const latest = progressRef.current;
      if (
        accepted &&
        latest?.jobId === current.jobId &&
        latest.status === "running"
      )
        applyProgress({ ...latest, cancelRequested: true });
      else if (
        !accepted &&
        latest?.jobId === current.jobId &&
        latest.status === "running"
      ) {
        setCancelPending(false);
        setFeedback({
          kind: "cancel-unavailable",
          meetingId: current.meetingId,
        });
      } else setCancelPending(false);
      await poll();
    } catch (failure) {
      if (mounted.current) {
        setCancelPending(false);
        setPollError(errorMessage(failure));
      }
    }
  }, [api, cancelPending, poll, applyProgress]);
  const dismiss = () => {
    if (progress?.status === "running") return;
    dismissed.current = progress?.jobId ?? null;
    setProgress(null);
    setFeedback(null);
    setPollError(null);
    started.current = null;
  };
  /** Discard a confirmed inactive meeting's retry/progress references and stale polls. */
  const forget = (meetingId: string, erasureConfirmed = false) => {
    if (
      !erasureConfirmed &&
      progressRef.current?.meetingId === meetingId &&
      progressRef.current.status === "running"
    )
      return;
    forgotten.current.add(meetingId);
    if (lastRequest.current?.meetingId === meetingId)
      lastRequest.current = null;
    if (progressRef.current?.meetingId === meetingId) {
      progressRef.current = null;
      expected.current = null;
      observed.current = null;
      started.current = null;
      setProgress(null);
      setFeedback(null);
      setPollError(null);
      setElapsedMs(0);
      setCancelPending(false);
      setLocalBusy(false);
      localBusyRef.current = false;
    }
    setFeedback((current) =>
      current?.meetingId === meetingId ? null : current,
    );
  };
  return {
    busy,
    progress,
    elapsedMs,
    cancelPending,
    feedback,
    pollError,
    run,
    cancel,
    dismiss,
    forget,
    lastRequest: lastRequest.current,
  };
}
