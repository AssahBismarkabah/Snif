import pytest
from math_utils import clamp, lerp


def test_clamp_within_range():
    assert clamp(5, 0, 10) == 5


def test_clamp_below_min():
    assert clamp(-1, 0, 10) == 0


def test_clamp_above_max():
    assert clamp(15, 0, 10) == 10


def test_lerp_zero():
    assert lerp(0, 10, 0.0) == 0.0


def test_lerp_one():
    assert lerp(0, 10, 1.0) == 10.0


def test_lerp_midpoint():
    assert lerp(0, 10, 0.5) == pytest.approx(5.0)
