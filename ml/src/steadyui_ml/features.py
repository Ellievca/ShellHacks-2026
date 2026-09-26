"""Deterministic movement feature extraction for a single interaction."""

from __future__ import annotations

from collections.abc import Sequence
from itertools import pairwise
from math import atan2, hypot, pi
from statistics import fmean, pstdev

from .schema import CursorSample, MovementFeatures, Target


def calculate_movement_features(
    samples: Sequence[CursorSample], target: Target
) -> MovementFeatures:
    """Calculate features from chronologically ordered, viewport-relative samples."""
    if not samples:
        raise ValueError("at least one cursor sample is required")

    segments = list(pairwise(samples))
    distances = [hypot(current.x - previous.x, current.y - previous.y) for previous, current in segments]
    durations_s = [(current.time_ms - previous.time_ms) / 1000 for previous, current in segments]
    if any(duration <= 0 for duration in durations_s):
        raise ValueError("cursor samples must have strictly increasing time_ms")

    speeds = [
        current.velocity_px_s
        if current.velocity_px_s is not None
        else distance / duration
        for (_, current), distance, duration in zip(segments, distances, durations_s)
    ]
    path_length = sum(distances)
    displacement = hypot(samples[-1].x - samples[0].x, samples[-1].y - samples[0].y)
    accelerations = [
        (current_speed - previous_speed) / duration
        for previous_speed, current_speed, duration in zip(speeds, speeds[1:], durations_s[1:])
    ]
    headings = [atan2(current.y - previous.y, current.x - previous.x) for previous, current in segments]
    direction_changes = sum(
        1
        for previous_heading, current_heading in pairwise(headings)
        if abs((current_heading - previous_heading + pi) % (2 * pi) - pi) > pi / 4
    )
    target_x, target_y = target.bounds.center
    final_target_distance = hypot(samples[-1].x - target_x, samples[-1].y - target_y)

    return MovementFeatures(
        sample_count=len(samples),
        duration_ms=samples[-1].time_ms - samples[0].time_ms,
        path_length_px=path_length,
        displacement_px=displacement,
        straightness=displacement / path_length if path_length else 0.0,
        mean_speed_px_s=fmean(speeds) if speeds else 0.0,
        max_speed_px_s=max(speeds, default=0.0),
        speed_std_px_s=pstdev(speeds) if len(speeds) > 1 else 0.0,
        mean_acceleration_px_s2=fmean(accelerations) if accelerations else 0.0,
        max_acceleration_px_s2=max((abs(value) for value in accelerations), default=0.0),
        direction_changes=direction_changes,
        final_target_distance_px=final_target_distance,
    )
