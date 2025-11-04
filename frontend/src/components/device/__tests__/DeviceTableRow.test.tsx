import { render, screen, fireEvent } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceTableRow } from '../DeviceTableRow';
import { StateDevice, Grouping } from '../types';

const mockDevice: StateDevice = {
  id: '1',
  uid: 12345,
  name: 'Test Device',
  status: 'Connected',
  note: 'Test note line 1\nTest note line 2\nTest note line 3',
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
  {
    id: 'group2',
    name: 'Another Group',
    device_ids: ['1', '2'],
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

describe('DeviceTableRow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders device name in table row', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('displays connect and remove buttons', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    expect(screen.getByText('connect')).toBeInTheDocument();
    expect(screen.getByText('remove')).toBeInTheDocument();
  });

  it('displays formatted last state change', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    expect(screen.getByText('formatted date')).toBeInTheDocument();
    expect(defaultProps.formatLastStateChange).toHaveBeenCalled();
  });

  it('displays firmware version', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
  });

  it('shows grouping badges when device belongs to groups', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    expect(screen.getByText('Test Group')).toBeInTheDocument();
    expect(screen.getByText('Another Group')).toBeInTheDocument();
  });

  it('handles connected device', () => {
    const connectedDevice = { ...mockDevice, status: 'Connected' };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={connectedDevice} /></tbody></table>);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('handles disconnected device', () => {
    const disconnectedDevice = { ...mockDevice, status: 'Disconnected' };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={disconnectedDevice} /></tbody></table>);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('displays warning message for invalid device', () => {
    const invalidDevice = { ...mockDevice, valid: false };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={invalidDevice} /></tbody></table>);
    expect(screen.getByText('no_keys')).toBeInTheDocument();
  });

  it('does not display warning for valid device', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    expect(screen.queryByText('no_keys')).not.toBeVisible();
  });

  it('displays short note correctly', () => {
    const shortNoteDevice = { ...mockDevice, note: 'Short note' };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={shortNoteDevice} /></tbody></table>);
    expect(screen.getByText('Short note')).toBeInTheDocument();
  });

  it('handles empty note without errors', () => {
    const emptyNoteDevice = { ...mockDevice, note: '' };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={emptyNoteDevice} /></tbody></table>);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
  });

  it('shows expand/collapse link for long notes', () => {
    const longNoteDevice = {
      ...mockDevice,
      note: 'Line 1\nLine 2\nLine 3\nLine 4'
    };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={longNoteDevice} /></tbody></table>);
    expect(screen.getByText('show_more')).toBeInTheDocument();
  });

  it('expands note when clicking show more', () => {
    const longNoteDevice = {
      ...mockDevice,
      note: 'Line 1\nLine 2\nLine 3'
    };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={longNoteDevice} /></tbody></table>);

    const showMoreLink = screen.getByText('show_more');
    fireEvent.click(showMoreLink);

    expect(screen.getByText('show_less')).toBeInTheDocument();
  });

  it('calls onConnect when connect button is clicked', async () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    const connectButton = screen.getByText('connect');
    fireEvent.click(connectButton);
    expect(defaultProps.onConnect).toHaveBeenCalledWith(mockDevice);
  });

  it('calls onDelete when remove button is clicked', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    const removeButton = screen.getByText('remove');
    fireEvent.click(removeButton);
    expect(defaultProps.onDelete).toHaveBeenCalledWith(mockDevice);
  });

  it('calls onEditNote when edit button is clicked', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    const buttons = screen.getAllByRole('button');
    // Edit button is the last button (icon button)
    fireEvent.click(buttons[buttons.length - 1]);
    expect(defaultProps.onEditNote).toHaveBeenCalledWith(mockDevice, 0);
  });

  it('disables connect button when connection not possible', () => {
    const connectionPossible = vi.fn(() => false);
    render(<table><tbody><DeviceTableRow {...defaultProps} connectionPossible={connectionPossible} /></tbody></table>);
    const connectButton = screen.getByText('connect');
    expect(connectButton).toBeDisabled();
  });

  it('enables connect button when connection is possible', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} /></tbody></table>);
    const connectButton = screen.getByText('connect');
    expect(connectButton).not.toBeDisabled();
  });

  it('renders without groupings', () => {
    render(<table><tbody><DeviceTableRow {...defaultProps} groupings={[]} /></tbody></table>);
    expect(screen.getByText('Test Device')).toBeInTheDocument();
    expect(screen.queryByText('Test Group')).not.toBeInTheDocument();
  });

  it('does not display badges when device is not in any grouping', () => {
    const deviceNotInGroup: StateDevice = { ...mockDevice, id: '999' };
    render(<table><tbody><DeviceTableRow {...defaultProps} device={deviceNotInGroup} /></tbody></table>);
    expect(screen.queryByText('Test Group')).not.toBeInTheDocument();
    expect(screen.queryByText('Another Group')).not.toBeInTheDocument();
  });

  it('renders multiple grouping badges', () => {
    const manyGroupings: Grouping[] = [
      { id: 'g1', name: 'Group 1', device_ids: ['1'] },
      { id: 'g2', name: 'Group 2', device_ids: ['1'] },
      { id: 'g3', name: 'Group 3', device_ids: ['1'] },
    ];

    render(<table><tbody><DeviceTableRow {...defaultProps} groupings={manyGroupings} /></tbody></table>);
    expect(screen.getByText('Group 1')).toBeInTheDocument();
    expect(screen.getByText('Group 2')).toBeInTheDocument();
    expect(screen.getByText('Group 3')).toBeInTheDocument();
  });
});
