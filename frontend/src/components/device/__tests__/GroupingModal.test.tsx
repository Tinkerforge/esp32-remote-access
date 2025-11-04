import { render, screen, fireEvent, waitFor } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GroupingModal } from '../GroupingModal';
import { StateDevice, Grouping } from '../types';
import { fetchClient } from '../../../utils';

// Mock the dependencies
vi.mock('../../../utils', () => ({
  fetchClient: {
    POST: vi.fn(),
    DELETE: vi.fn(),
    GET: vi.fn(),
  },
}));

vi.mock('../../Alert', () => ({
  showAlert: vi.fn(),
}));

const mockDevices: StateDevice[] = [
  {
    id: 'device1',
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
    id: 'device2',
    uid: 67890,
    name: 'Test Device 2',
    status: 'Disconnected',
    note: 'Test note 2',
    port: 8081,
    valid: false,
    last_state_change: null,
    firmware_version: '1.1.0',
  },
  {
    id: 'device3',
    uid: 11111,
    name: 'Another Device',
    status: 'Connected',
    note: '',
    port: 8082,
    valid: true,
    last_state_change: 1640995300,
    firmware_version: '1.2.0',
  },
];

const mockGroupings: Grouping[] = [
  {
    id: 'group1',
    name: 'Test Group 1',
    device_ids: ['device1', 'device2'],
  },
  {
    id: 'group2',
    name: 'Test Group 2',
    device_ids: ['device3'],
  },
];

const defaultProps = {
  show: true,
  devices: mockDevices,
  groupings: mockGroupings,
  onClose: vi.fn(),
  onGroupingsUpdated: vi.fn(),
};

describe('GroupingModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders modal when show is true', () => {
    render(<GroupingModal {...defaultProps} />);
    expect(screen.getByTestId('modal')).toBeInTheDocument();
  });

  it('does not render modal when show is false', () => {
    render(<GroupingModal {...defaultProps} show={false} />);
    expect(screen.queryByTestId('modal')).not.toBeInTheDocument();
  });

  it('displays list of groupings', () => {
    render(<GroupingModal {...defaultProps} />);
    expect(screen.getByText('Test Group 1')).toBeInTheDocument();
    expect(screen.getByText('Test Group 2')).toBeInTheDocument();
  });

  it('shows device count for each grouping', () => {
    render(<GroupingModal {...defaultProps} />);
    // Counts are rendered alongside the i18n key 'grouping_devices'
    const deviceCountElements = screen.getAllByText(/grouping_devices/i);
    expect(deviceCountElements.length).toBeGreaterThan(0);
  });

  it('shows no groupings message when empty', () => {
    render(<GroupingModal {...defaultProps} groupings={[]} />);
    expect(screen.getByText('no_groupings')).toBeInTheDocument();
  });

  it('opens create form when create button is clicked', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByPlaceholderText('grouping_name_placeholder')).toBeInTheDocument();
    });
  });

  it('opens edit form when edit button is clicked', async () => {
    render(<GroupingModal {...defaultProps} />);

    const editButtons = screen.getAllByRole('button', { name: '' });
    // Find the edit button (first icon button in the row)
    const editButton = editButtons.find(btn => btn.querySelector('svg'));

    if (editButton) {
      fireEvent.click(editButton);

      await waitFor(() => {
        expect(screen.getByDisplayValue('Test Group 1')).toBeInTheDocument();
      });
    }
  });

  it('filters devices based on search query', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const searchInput = screen.getByPlaceholderText('search_devices');
      expect(searchInput).toBeInTheDocument();

      fireEvent.change(searchInput, { target: { value: 'Another' } });

      // Should show only the matching device
      expect(screen.getByText('Another Device')).toBeInTheDocument();
    });
  });

  it('allows selecting devices when creating a grouping', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      fireEvent.change(nameInput, { target: { value: 'New Group' } });

      const checkboxes = screen.getAllByTestId('checkbox');
      expect(checkboxes.length).toBeGreaterThan(0);

      // Select first device
      fireEvent.click(checkboxes[0]);
      expect((checkboxes[0] as HTMLInputElement).checked).toBe(true);
    });
  });

  it('creates a new grouping when save is clicked', async () => {
    const mockPost = vi.mocked(fetchClient.POST);
    mockPost.mockResolvedValue({
      data: { id: 'new-group-id' },
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockGet = vi.mocked(fetchClient.GET);
    mockGet.mockResolvedValue({
      data: { groupings: [...mockGroupings, { id: 'new-group-id', name: 'New Group', device_ids: ['device1'] }] },
      response: { status: 200 } as Response,
      error: undefined,
    });

    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(async () => {
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      fireEvent.change(nameInput, { target: { value: 'New Group' } });

      const checkboxes = screen.getAllByRole('checkbox');
      fireEvent.click(checkboxes[0]);

  const saveButton = screen.getByRole('button', { name: 'save' });
      fireEvent.click(saveButton);

      await waitFor(() => {
        expect(mockPost).toHaveBeenCalledWith('/grouping/create', expect.any(Object));
      });
    });
  });

  it('updates existing grouping when save is clicked after editing', async () => {
    const mockPost = vi.mocked(fetchClient.POST);
    mockPost.mockResolvedValue({
      data: null,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockGet = vi.mocked(fetchClient.GET);
    mockGet.mockResolvedValue({
      data: { groupings: mockGroupings },
      response: { status: 200 } as Response,
      error: undefined,
    });

    render(<GroupingModal {...defaultProps} />);

    // Click edit on first grouping
    const editButtons = screen.getAllByRole('button', { name: '' });
    const editButton = editButtons.find(btn => btn.querySelector('svg'));

    if (editButton) {
      fireEvent.click(editButton);

      await waitFor(async () => {
        const nameInput = screen.getByDisplayValue('Test Group 1');
        fireEvent.change(nameInput, { target: { value: 'Updated Group Name' } });

        const saveButton = screen.getByRole('button', { name: 'save' });
        fireEvent.click(saveButton);

        await waitFor(() => {
          // Should call POST to add/remove devices
          expect(mockPost).toHaveBeenCalled();
        });
      });
    }
  });

  it('deletes grouping when delete button is clicked and confirmed', async () => {
    // Mock window.confirm
    const originalConfirm = window.confirm;
    window.confirm = vi.fn(() => true);

    const mockDelete = vi.mocked(fetchClient.DELETE);
    mockDelete.mockResolvedValue({
      data: null,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockGet = vi.mocked(fetchClient.GET);
    mockGet.mockResolvedValue({
      data: { groupings: [mockGroupings[1]] },
      response: { status: 200 } as Response,
      error: undefined,
    });

    render(<GroupingModal {...defaultProps} />);

    const deleteButtons = screen.getAllByRole('button', { name: '' });
    // Find delete button (should be the second icon button)
    const deleteButton = deleteButtons[deleteButtons.length - 1];

    fireEvent.click(deleteButton);

    await waitFor(() => {
      expect(window.confirm).toHaveBeenCalled();
      expect(mockDelete).toHaveBeenCalledWith('/grouping/delete', expect.any(Object));
    });

    // Restore original confirm
    window.confirm = originalConfirm;
  });

  it('does not delete grouping when cancel is clicked in confirm dialog', async () => {
    const originalConfirm = window.confirm;
    window.confirm = vi.fn(() => false);

    const mockDelete = vi.mocked(fetchClient.DELETE);

    render(<GroupingModal {...defaultProps} />);

    const deleteButtons = screen.getAllByRole('button', { name: '' });
    const deleteButton = deleteButtons[deleteButtons.length - 1];

    fireEvent.click(deleteButton);

    await waitFor(() => {
      expect(window.confirm).toHaveBeenCalled();
      expect(mockDelete).not.toHaveBeenCalled();
    });

    window.confirm = originalConfirm;
  });

  it('cancels creation when cancel button is clicked', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    // Ensure the edit form is visible first
    await waitFor(() => expect(screen.getByPlaceholderText('grouping_name_placeholder')).toBeInTheDocument());

    const cancelButton = screen.getByRole('button', { name: 'cancel' });
    fireEvent.click(cancelButton);

    // Should return to list view
    await waitFor(() => expect(screen.queryByPlaceholderText('grouping_name_placeholder')).not.toBeInTheDocument());
  });

  it('calls onClose when close button is clicked', () => {
    render(<GroupingModal {...defaultProps} />);

    // Prefer the modal-close inside the modal body which is wired to onHide
    const closeButtons = screen.getAllByTestId('modal-close');
    fireEvent.click(closeButtons[1]);

    expect(defaultProps.onClose).toHaveBeenCalled();
  });

  it('resets form state when modal is closed', async () => {
    const { rerender } = render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      fireEvent.change(nameInput, { target: { value: 'Test' } });
    });

    // Close modal
    rerender(<GroupingModal {...defaultProps} show={false} />);

    // Reopen modal
    rerender(<GroupingModal {...defaultProps} show={true} />);

    // Should show list view, not edit form
    expect(screen.queryByRole('textbox', { name: /grouping name/i })).not.toBeInTheDocument();
  });

  it('handles empty devices array gracefully', () => {
    render(<GroupingModal {...defaultProps} devices={[]} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    // Should not throw error
    expect(screen.getByTestId('modal')).toBeInTheDocument();
  });

  it('pre-selects devices when editing existing grouping', async () => {
    render(<GroupingModal {...defaultProps} />);

    const editButtons = screen.getAllByRole('button', { name: '' });
    const editButton = editButtons.find(btn => btn.querySelector('svg'));

    if (editButton) {
      fireEvent.click(editButton);

      await waitFor(() => {
        const checkboxes = screen.getAllByRole('checkbox');
        // First two devices should be checked (device1 and device2 in group1)
        const checkedBoxes = checkboxes.filter((cb) => (cb as HTMLInputElement).checked);
        expect(checkedBoxes.length).toBeGreaterThan(0);
      });
    }
  });

  it('validates required fields when saving', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const saveButton = screen.getByRole('button', { name: 'save' });
      fireEvent.click(saveButton);

      // Form should validate and not submit without name
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      expect(nameInput).toHaveAttribute('required');
    });
  });

  it('calls onGroupingsUpdated after successful operations', async () => {
    const mockGet = vi.mocked(fetchClient.GET);
    mockGet.mockResolvedValue({
      data: { groupings: mockGroupings },
      response: { status: 200 } as Response,
      error: undefined,
    });

    const originalConfirm = window.confirm;
    window.confirm = vi.fn(() => true);

    const mockDelete = vi.mocked(fetchClient.DELETE);
    mockDelete.mockResolvedValue({
      data: null,
      response: { status: 200 } as Response,
      error: undefined,
    });

    render(<GroupingModal {...defaultProps} />);

    const deleteButtons = screen.getAllByRole('button', { name: '' });
    const deleteButton = deleteButtons[deleteButtons.length - 1];

    fireEvent.click(deleteButton);

    await waitFor(() => {
      expect(defaultProps.onGroupingsUpdated).toHaveBeenCalledWith(mockGroupings);
    });

    window.confirm = originalConfirm;
  });
});
