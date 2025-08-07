import { render } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceCard } from '../DeviceCard';
import { StateDevice } from '../types';

const mockDevice: StateDevice = {
  id: '1',
  uid: 12345,
  name: 'Test Device',
  status: 'Connected',
  note: 'Test note line 1\nTest note line 2\nTest note line 3\nTest note line 4',
  port: 8080,
  valid: true,
  last_state_change: 1640995200,
  firmware_version: '1.0.0',
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

describe('DeviceCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders device card', () => {
    const { container } = render(<DeviceCard {...defaultProps} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles connected device', () => {
    const connectedDevice = { ...mockDevice, status: 'Connected' };
    const { container } = render(<DeviceCard {...defaultProps} device={connectedDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles disconnected device', () => {
    const disconnectedDevice = { ...mockDevice, status: 'Disconnected' };
    const { container } = render(<DeviceCard {...defaultProps} device={disconnectedDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles invalid device', () => {
    const invalidDevice = { ...mockDevice, valid: false };
    const { container } = render(<DeviceCard {...defaultProps} device={invalidDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with short note', () => {
    const shortNoteDevice = { ...mockDevice, note: 'Short note' };
    const { container } = render(<DeviceCard {...defaultProps} device={shortNoteDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with long note', () => {
    const longNoteDevice = {
      ...mockDevice,
      note: 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8'
    };
    const { container } = render(<DeviceCard {...defaultProps} device={longNoteDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with empty note', () => {
    const emptyNoteDevice = { ...mockDevice, note: '' };
    const { container } = render(<DeviceCard {...defaultProps} device={emptyNoteDevice} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles device with no last state change', () => {
    const noStateChangeDevice = { ...mockDevice, last_state_change: null };
    const { container } = render(<DeviceCard {...defaultProps} device={noStateChangeDevice} />);
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
      <DeviceCard {...defaultProps} {...callbacks} />
    );

    expect(container.firstChild).toBeTruthy();
    // Callbacks should not be called during render
    expect(callbacks.onConnect).not.toHaveBeenCalled();
    expect(callbacks.onDelete).not.toHaveBeenCalled();
    expect(callbacks.onEditNote).not.toHaveBeenCalled();
  });

  it('handles connection not possible', () => {
    const connectionPossible = vi.fn(() => false);
    const { container } = render(
      <DeviceCard {...defaultProps} connectionPossible={connectionPossible} />
    );

    expect(container.firstChild).toBeTruthy();
    // connectionPossible might be called during render to determine UI state
  });

  it('uses formatLastStateChange function', () => {
    const formatFn = vi.fn(() => 'custom format');
    const { container } = render(
      <DeviceCard {...defaultProps} formatLastStateChange={formatFn} />
    );

    expect(container.firstChild).toBeTruthy();
    // formatLastStateChange should be called during render
    expect(formatFn).toHaveBeenCalled();
  });

  it('handles different device indices', () => {
    const { container: container1 } = render(<DeviceCard {...defaultProps} index={0} />);
    expect(container1.firstChild).toBeTruthy();

    const { container: container2 } = render(<DeviceCard {...defaultProps} index={5} />);
    expect(container2.firstChild).toBeTruthy();
  });

  it('handles minimal device configuration', () => {
    const minimalDevice: StateDevice = {
      id: '1',
      uid: 1,
      name: 'Device',
      status: 'Connected',
      note: '',
      port: 80,
      valid: true,
      last_state_change: null,
      firmware_version: '1.0.0',
    };

    const { container } = render(<DeviceCard {...defaultProps} device={minimalDevice} />);
    expect(container.firstChild).toBeTruthy();
  });
});
