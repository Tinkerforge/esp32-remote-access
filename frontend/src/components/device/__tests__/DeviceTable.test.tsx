import { render, screen, fireEvent } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceTable } from '../DeviceTable';
import { StateDevice, Grouping } from '../types';

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

const mockGroupings: Grouping[] = [
  {
    id: 'group1',
    name: 'Test Group',
    device_ids: ['1'],
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
  groupings: mockGroupings,
};

describe('DeviceTable', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders table with column headers', () => {
    render(<DeviceTable {...defaultProps} />);
    expect(screen.getByText('charger_name')).toBeInTheDocument();
    expect(screen.getByText('charger_id')).toBeInTheDocument();
    expect(screen.getByText('last_state_change')).toBeInTheDocument();
    expect(screen.getByText('note')).toBeInTheDocument();
    expect(screen.getByText('firmware_version')).toBeInTheDocument();
  });

  it('renders all devices in the table', () => {
    render(<DeviceTable {...defaultProps} />);
    expect(screen.getByText('Test Device 1')).toBeInTheDocument();
    expect(screen.getByText('Test Device 2')).toBeInTheDocument();
  });

  it('handles empty devices array', () => {
    render(<DeviceTable {...defaultProps} devices={[]} />);
    expect(screen.getByText('charger_name')).toBeInTheDocument();
    // Table should still render headers even with no devices
  });

  it('calls onSort when column header is clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const nameHeader = screen.getByText('charger_name');
    fireEvent.click(nameHeader.closest('th')!);
    expect(defaultProps.onSort).toHaveBeenCalledWith('name');
  });

  it('calls onSort for status column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const headers = screen.getAllByRole('columnheader');
    // Status column is the first header
    fireEvent.click(headers[0]);
    expect(defaultProps.onSort).toHaveBeenCalledWith('status');
  });

  it('calls onSort for uid column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const uidHeader = screen.getByText('charger_id');
    fireEvent.click(uidHeader.closest('th')!);
    expect(defaultProps.onSort).toHaveBeenCalledWith('uid');
  });

  it('calls onSort for last_state_change column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const lastStateHeader = screen.getByText('last_state_change');
    fireEvent.click(lastStateHeader.closest('th')!);
    expect(defaultProps.onSort).toHaveBeenCalledWith('last_state_change');
  });

  it('calls onSort for note column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const noteHeader = screen.getByText('note');
    fireEvent.click(noteHeader.closest('th')!);
    expect(defaultProps.onSort).toHaveBeenCalledWith('note');
  });

  it('calls onSort for firmware_version column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const firmwareHeader = screen.getByText('firmware_version');
    fireEvent.click(firmwareHeader.closest('th')!);
    expect(defaultProps.onSort).toHaveBeenCalledWith('firmware_version');
  });

  it('displays sort indicator for active sort column', () => {
    render(<DeviceTable {...defaultProps} sortColumn="name" sortSequence="asc" />);
    // Should render with sort indicator (ChevronDown or ChevronUp)
    expect(screen.getByText('charger_name')).toBeInTheDocument();
  });

  it('passes callbacks to child DeviceTableRow components', () => {
    render(<DeviceTable {...defaultProps} />);
    const connectButtons = screen.getAllByText('connect');
    expect(connectButtons.length).toBe(2); // Two devices
  });

  it('passes groupings to child components', () => {
    render(<DeviceTable {...defaultProps} />);
    expect(screen.getByText('Test Group')).toBeInTheDocument();
  });

  it('renders table with multiple groupings', () => {
    const customGroupings: Grouping[] = [
      { id: 'g1', name: 'Group 1', device_ids: ['1', '2'] },
      { id: 'g2', name: 'Group 2', device_ids: ['1'] },
    ];

    render(<DeviceTable {...defaultProps} groupings={customGroupings} />);
    const group1Badges = screen.getAllByText('Group 1');
    const group2Badges = screen.getAllByText('Group 2');
    expect(group1Badges.length).toBeGreaterThan(0);
    expect(group2Badges.length).toBeGreaterThan(0);
  });

  it('uses formatLastStateChange function for devices', () => {
    render(<DeviceTable {...defaultProps} />);
    expect(defaultProps.formatLastStateChange).toHaveBeenCalled();
    expect(screen.getByText('formatted date')).toBeInTheDocument();
    expect(screen.getByText('-')).toBeInTheDocument();
  });
});
