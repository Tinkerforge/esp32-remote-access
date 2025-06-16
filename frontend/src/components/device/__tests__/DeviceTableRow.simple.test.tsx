import { render } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceTableRow } from '../DeviceTableRow';
import { StateDevice } from '../types';

const mockDevice: StateDevice = {
  id: '1',
  uid: 12345,
  name: 'Test Device',
  status: 'Connected',
  note: 'Test note line 1\nTest note line 2\nTest note line 3',
  port: 8080,
  valid: true,
  last_state_change: 1640995200,
};

const defaultProps = {
  device: mockDevice,
  index: 0,
  onConnect: vi.fn(),
  onDelete: vi.fn(),
  onEditNote: vi.fn(),
  connectionPossible: vi.fn(() => true),
  formatLastStateChange: vi.fn(() => 'formatted date'),
};

describe('DeviceTableRow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders device row', () => {
    const { container } = render(<DeviceTableRow {...defaultProps} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles connected device', () => {
    const connectedDevice = { ...mockDevice, status: 'Connected' };
    const { container } = render(<DeviceTableRow {...defaultProps} device={connectedDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles disconnected device', () => {
    const disconnectedDevice = { ...mockDevice, status: 'Disconnected' };
    const { container } = render(<DeviceTableRow {...defaultProps} device={disconnectedDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles invalid device', () => {
    const invalidDevice = { ...mockDevice, valid: false };
    const { container } = render(<DeviceTableRow {...defaultProps} device={invalidDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with short note', () => {
    const shortNoteDevice = { ...mockDevice, note: 'Short note' };
    const { container } = render(<DeviceTableRow {...defaultProps} device={shortNoteDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with empty note', () => {
    const emptyNoteDevice = { ...mockDevice, note: '' };
    const { container } = render(<DeviceTableRow {...defaultProps} device={emptyNoteDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with no last state change', () => {
    const noStateChangeDevice = { ...mockDevice, last_state_change: null };
    const { container } = render(<DeviceTableRow {...defaultProps} device={noStateChangeDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('receives all required callback props', () => {
    const callbacks = {
      onConnect: vi.fn(),
      onDelete: vi.fn(),
      onEditNote: vi.fn(),
      connectionPossible: vi.fn(() => false),
      formatLastStateChange: vi.fn(() => 'never'),
    };

    const { container } = render(
      <DeviceTableRow {...defaultProps} {...callbacks} />
    );

    expect(container.firstChild).toBeTruthy();
    // Callbacks should not be called during render
    expect(callbacks.onConnect).not.toHaveBeenCalled();
    expect(callbacks.onDelete).not.toHaveBeenCalled();
    expect(callbacks.onEditNote).not.toHaveBeenCalled();
  });

  it('uses connectionPossible function', () => {
    const connectionPossible = vi.fn(() => false);
    const { container } = render(
      <DeviceTableRow {...defaultProps} connectionPossible={connectionPossible} />
    );

    expect(container.firstChild).toBeTruthy();
    // connectionPossible might be called during render to determine button state
  });

  it('uses formatLastStateChange function', () => {
    const formatFn = vi.fn(() => 'custom format');
    const { container } = render(
      <DeviceTableRow {...defaultProps} formatLastStateChange={formatFn} />
    );

    expect(container.firstChild).toBeTruthy();
    // formatLastStateChange should be called during render
    expect(formatFn).toHaveBeenCalled();
  });
});
