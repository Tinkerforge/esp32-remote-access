import { render, screen, fireEvent } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceCard } from '../DeviceCard';
import { StateDevice, Grouping } from '../types';

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

const mockGroupings: Grouping[] = [
  {
    id: 'group1',
    name: 'Test Group',
    device_ids: ['1'],
  },
];

const defaultProps = {
  device: mockDevice,
  index: 0,
  onConnect: vi.fn(),
  onDelete: vi.fn(),
  onEditNote: vi.fn(),
  connectionPossible: vi.fn(() => true),
  formatLastStateChange: vi.fn(() => 'formatted date'),
  groupings: mockGroupings,
};

describe('DeviceCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders device card with device name', () => {
    render(<DeviceCard {...defaultProps} />);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('displays connect and delete buttons', () => {
    render(<DeviceCard {...defaultProps} />);
    const buttons = screen.getAllByRole('button');
    expect(buttons.length).toBeGreaterThanOrEqual(2);
  });

  it('displays formatted last state change', () => {
    render(<DeviceCard {...defaultProps} />);
    expect(screen.getByText('formatted date')).toBeInTheDocument();
    expect(defaultProps.formatLastStateChange).toHaveBeenCalled();
  });

  it('displays firmware version', () => {
    render(<DeviceCard {...defaultProps} />);
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
  });

  it('shows grouping badge when device belongs to a group', () => {
    render(<DeviceCard {...defaultProps} />);
    expect(screen.getByText('Test Group')).toBeInTheDocument();
  });

  it('handles connected device with success indicator', () => {
    const connectedDevice = { ...mockDevice, status: 'Connected' };
    render(<DeviceCard {...defaultProps} device={connectedDevice} />);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('handles disconnected device', () => {
    const disconnectedDevice = { ...mockDevice, status: 'Disconnected' };
    render(<DeviceCard {...defaultProps} device={disconnectedDevice} />);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('displays warning message for invalid device', () => {
    const invalidDevice = { ...mockDevice, valid: false };
    render(<DeviceCard {...defaultProps} device={invalidDevice} />);
    expect(screen.getByText('no_keys')).toBeInTheDocument();
  });

  it('does not display warning for valid device', () => {
    render(<DeviceCard {...defaultProps} />);
    expect(screen.queryByText('no_keys')).not.toBeVisible();
  });

  it('displays short note correctly', () => {
    const shortNoteDevice = { ...mockDevice, note: 'Short note' };
    render(<DeviceCard {...defaultProps} device={shortNoteDevice} />);
    expect(screen.getByText('Short note')).toBeInTheDocument();
  });

  it('displays empty note without errors', () => {
    const emptyNoteDevice = { ...mockDevice, note: '' };
    render(<DeviceCard {...defaultProps} device={emptyNoteDevice} />);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('shows expand/collapse link for long notes', () => {
    const longNoteDevice = {
      ...mockDevice,
      note: 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5'
    };
    render(<DeviceCard {...defaultProps} device={longNoteDevice} />);
    expect(screen.getByText('show_more')).toBeInTheDocument();
  });

  it('expands note when clicking show more', () => {
    const longNoteDevice = {
      ...mockDevice,
      note: 'Line 1\nLine 2\nLine 3\nLine 4'
    };
    render(<DeviceCard {...defaultProps} device={longNoteDevice} />);

    const showMoreLink = screen.getByText('show_more');
    fireEvent.click(showMoreLink);

    expect(screen.getByText('show_less')).toBeInTheDocument();
  });

  it('displays edit note button', () => {
    render(<DeviceCard {...defaultProps} />);
    const buttons = screen.getAllByRole('button');
    // Edit button should be present (icon button)
    expect(buttons.length).toBeGreaterThanOrEqual(3);
  });

  it('calls onEditNote when edit button is clicked', () => {
    render(<DeviceCard {...defaultProps} />);
    const buttons = screen.getAllByRole('button');
    // Find and click edit button (should be the third button)
    fireEvent.click(buttons[2]);
    expect(defaultProps.onEditNote).toHaveBeenCalledWith(mockDevice, 0);
  });

  it('calls onDelete when delete button is clicked', () => {
    render(<DeviceCard {...defaultProps} />);
    const buttons = screen.getAllByRole('button');
    // Delete button should be the second button in header
    fireEvent.click(buttons[1]);
    expect(defaultProps.onDelete).toHaveBeenCalledWith(mockDevice);
  });

  it('disables connect button when connection not possible', () => {
    const connectionPossible = vi.fn(() => false);
    render(<DeviceCard {...defaultProps} connectionPossible={connectionPossible} />);
    const buttons = screen.getAllByRole('button');
    // First button should be disabled
    expect(buttons[0]).toBeDisabled();
  });

  it('enables connect button when connection is possible', () => {
    render(<DeviceCard {...defaultProps} />);
    const buttons = screen.getAllByRole('button');
    // First button should be enabled
    expect(buttons[0]).not.toBeDisabled();
  });

  it('renders without groupings', () => {
    render(<DeviceCard {...defaultProps} groupings={[]} />);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
    expect(screen.queryByText('Test Group')).not.toBeInTheDocument();
  });

  it('does not display badges when device is not in any grouping', () => {
    const deviceNotInGroup: StateDevice = { ...mockDevice, id: '999' };
    render(<DeviceCard {...defaultProps} device={deviceNotInGroup} />);
    expect(screen.queryByText('Test Group')).not.toBeInTheDocument();
  });

  it('renders multiple grouping badges', () => {
    const manyGroupings: Grouping[] = [
      { id: 'g1', name: 'Group 1', device_ids: ['1'] },
      { id: 'g2', name: 'Group 2', device_ids: ['1'] },
      { id: 'g3', name: 'Group 3', device_ids: ['1'] },
    ];

    render(<DeviceCard {...defaultProps} groupings={manyGroupings} />);
    expect(screen.getByText('Group 1')).toBeInTheDocument();
    expect(screen.getByText('Group 2')).toBeInTheDocument();
    expect(screen.getByText('Group 3')).toBeInTheDocument();
  });
});
