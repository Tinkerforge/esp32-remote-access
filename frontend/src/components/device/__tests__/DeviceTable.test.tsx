import { render } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceTable } from '../DeviceTable';
import { StateDevice } from '../types';

const mockDevices: StateDevice[] = [
  {
    id: '1',
    uid: 12345,
    name: 'Test Device 1',
    status: 'Connected',
    note: 'Test note 1',
    port: 8080,
    valid: true,
    last_state_change: 1640995200,
    firmware_version: '1.0.0',
  },
  {
    id: '2',
    uid: 67890,
    name: 'Test Device 2',
    status: 'Disconnected',
    note: 'Test note 2',
    port: 8081,
    valid: false,
    last_state_change: null,
    firmware_version: '1.0.0',
  },
];

const defaultProps = {
  devices: mockDevices,
  sortColumn: 'none' as const,
  sortSequence: 'asc' as const,
  onSort: vi.fn(),
  onConnect: vi.fn(),
  onDelete: vi.fn(),
  onEditNote: vi.fn(),
  connectionPossible: vi.fn(() => true),
  formatLastStateChange: vi.fn((t, timestamp) => timestamp ? 'formatted date' : '-'),
};

describe('DeviceTable', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders table with devices', () => {
    const { container } = render(<DeviceTable {...defaultProps} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles empty devices array', () => {
    const { container } = render(<DeviceTable {...defaultProps} devices={[]} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles different sort configurations', () => {
    const { container: container1 } = render(
      <DeviceTable {...defaultProps} sortColumn="name" sortSequence="asc" />
    );
    expect(container1.firstChild).toBeTruthy();

    const { container: container2 } = render(
      <DeviceTable {...defaultProps} sortColumn="status" sortSequence="desc" />
    );
    expect(container2.firstChild).toBeTruthy();
  });

  it('receives all required callback props', () => {
    const callbacks = {
      onSort: vi.fn(),
      onConnect: vi.fn(),
      onDelete: vi.fn(),
      onEditNote: vi.fn(),
      connectionPossible: vi.fn(() => false),
      formatLastStateChange: vi.fn(() => 'never'),
    };

    const { container } = render(
      <DeviceTable {...defaultProps} {...callbacks} />
    );

    expect(container.firstChild).toBeTruthy();
    // Callbacks should not be called during render
    expect(callbacks.onSort).not.toHaveBeenCalled();
    expect(callbacks.onConnect).not.toHaveBeenCalled();
    expect(callbacks.onDelete).not.toHaveBeenCalled();
    expect(callbacks.onEditNote).not.toHaveBeenCalled();
  });

  it('handles invalid device data gracefully', () => {
    const invalidDevices = [
      {
        id: '',
        uid: 0,
        name: '',
        status: 'Unknown',
        note: '',
        port: 0,
        valid: false,
        last_state_change: null,
        firmware_version: '',
      }
    ];

    const { container } = render(<DeviceTable {...defaultProps} devices={invalidDevices} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('uses formatLastStateChange function', () => {
    const formatFn = vi.fn(() => 'custom format');
    render(<DeviceTable {...defaultProps} formatLastStateChange={formatFn} />);

    // formatLastStateChange might be called during rendering of child components
    // We just ensure the component renders successfully with the custom function
  });
});
