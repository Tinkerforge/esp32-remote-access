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
    is_default: false,
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
  searchTerm: '',
  onSearchChange: vi.fn(),
  selectedGroupingId: null,
  onGroupingFilterChange: vi.fn(),
  groupingSearchTerm: '',
  setGroupingSearchTerm: vi.fn(),
  groupByEnabled: true,
  onGroupByToggle: vi.fn(),
  onManageGroupingsClick: vi.fn(),
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
    const nameHeaderCell = nameHeader.closest('th');
    expect(nameHeaderCell).not.toBeNull();

    fireEvent.click(nameHeaderCell as HTMLElement);
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
    const thElement = uidHeader.closest('th');
    expect(thElement).not.toBeNull();
    fireEvent.click(thElement as HTMLElement);
    expect(defaultProps.onSort).toHaveBeenCalledWith('uid');
  });

  it('calls onSort for last_state_change column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const lastStateHeader = screen.getByText('last_state_change');
    const thElement = lastStateHeader.closest('th');
    expect(thElement).not.toBeNull();
    fireEvent.click(thElement as HTMLElement);
    expect(defaultProps.onSort).toHaveBeenCalledWith('last_state_change');
  });

  it('calls onSort for note column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const noteHeader = screen.getByText('note');
    const thElement = noteHeader.closest('th');
    expect(thElement).not.toBeNull();
    fireEvent.click(thElement as HTMLElement);
    expect(defaultProps.onSort).toHaveBeenCalledWith('note');
  });

  it('calls onSort for firmware_version column when clicked', () => {
    render(<DeviceTable {...defaultProps} />);
    const firmwareHeader = screen.getByText('firmware_version');
    const thElement = firmwareHeader.closest('th');
    expect(thElement).not.toBeNull();
    fireEvent.click(thElement as HTMLElement);
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
      { id: 'g1', name: 'Group 1', device_ids: ['1', '2'], is_default: false },
      { id: 'g2', name: 'Group 2', device_ids: ['1'], is_default: false },
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

  // --- Bundled-by-groups view ---

  it('renders groups as collapsed sections when bundleByGroups is true', () => {
    render(<DeviceTable {...defaultProps} bundleByGroups={true} />);
    const header = screen.getByText('Test Group');
    expect(header).toBeInTheDocument();
    expect(screen.queryByText('Test Device 1')).not.toBeInTheDocument();
  });

  it('expands a group section and shows its devices when the header is clicked', () => {
        render(<DeviceTable {...defaultProps} bundleByGroups={true} />);
        const header = screen.getByText('Test Group');
        fireEvent.click(header.closest('tr') as HTMLElement);
        expect(screen.getByText('Test Device 1')).toBeInTheDocument();

        expect(screen.queryByText('Test Device 2')).not.toBeInTheDocument();
      });

    it('renders an Ungrouped section header for devices not in any group', () => {
        render(<DeviceTable {...defaultProps} bundleByGroups={true} />);
        expect(screen.queryByText('Test Device 2')).not.toBeInTheDocument();
        expect(screen.getByText('no_group')).toBeInTheDocument();
      });

    it('expands the Ungrouped section and reveals its devices when clicked', () => {
        render(<DeviceTable {...defaultProps} bundleByGroups={true} />);
        const header = screen.getByText('no_group').closest('tr') as HTMLElement;
        fireEvent.click(header);
        expect(screen.getByText('Test Device 2')).toBeInTheDocument();
      });

  it('keeps the flat layout when bundleByGroups is false', () => {
      render(<DeviceTable {...defaultProps} bundleByGroups={false} />);
      expect(screen.getByText('Test Device 1')).toBeInTheDocument();
      expect(screen.getByText('Test Device 2')).toBeInTheDocument();
    });

    it('uses a darker gray for collapsed group headers and a lighter gray when expanded', () => {
        render(<DeviceTable {...defaultProps} bundleByGroups={true} />);
        const header = screen.getByText('Test Group').closest('tr') as HTMLElement;
        expect(header.style.background).toBe('rgb(206, 212, 218)');

        fireEvent.click(header);
        expect(header.style.background).toBe('rgb(233, 236, 239)');
      });

      it('renders a colgroup that locks column widths', () => {
        const { container } = render(<DeviceTable {...defaultProps} />);
        const cols = container.querySelectorAll('colgroup col');
              expect(cols.length).toBe(7);
              expect(cols[0].className).toContain('charger-col-status');
              expect(cols[1].className).toContain('charger-col-name');
              expect(cols[2].className).toContain('charger-col-uid');
              expect(cols[3].className).toContain('charger-col-actions');
              expect(cols[4].className).toContain('charger-col-state-change');
              expect(cols[5].className).toContain('charger-col-note');
              expect(cols[6].className).toContain('charger-col-firmware');
              const table = container.querySelector('table');
              expect(table?.classList.contains('charger-table')).toBe(true);
            });

            it('locks each <th> width with an inline style as a fallback', () => {
              const { container } = render(<DeviceTable {...defaultProps} />);
              const ths = Array.from(container.querySelectorAll('thead th'));
              expect(ths).toHaveLength(7);
              expect((ths[0] as HTMLElement).style.width).toBe('60px');
              expect((ths[1] as HTMLElement).style.width).toBe('auto');
              expect((ths[2] as HTMLElement).style.width).toBe('110px');
              expect((ths[3] as HTMLElement).style.width).toBe('220px');
              expect((ths[4] as HTMLElement).style.width).toBe('160px');
              expect((ths[5] as HTMLElement).style.width).toBe('50%');
              expect((ths[6] as HTMLElement).style.width).toBe('130px');
            });
          });
